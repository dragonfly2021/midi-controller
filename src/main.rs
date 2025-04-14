#![no_std]
#![no_main]

//use defmt::info;
use ads1x1x::{Ads1x1x, ComparatorLatching, TargetAddr, channel};
use defmt::{debug, info};
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::i2c::{self, Blocking};
use embassy_rp::peripherals::I2C0;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embedded_hal_async::delay::DelayNs;
use libm::round;
use nb::block;
use {defmt_rtt as _, panic_probe as _};

// Type aliases
type Adc<'a, M> = Ads1x1x<
    i2c::I2c<'static, I2C0, Blocking>,
    ads1x1x::ic::Ads1115,
    ads1x1x::ic::Resolution16Bit,
    M,
>;

const THRESHOLD: i16 = 30;

enum Slider {
    //   MoveSlider,
    ReadSlider,
}

static SLIDER1_EVENT: Signal<CriticalSectionRawMutex, Slider> = Signal::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    debug!("Performing startup.");

    let p = embassy_rp::init(Default::default());

    let sda = p.PIN_20;
    let scl = p.PIN_21;
    let config = embassy_rp::i2c::Config::default();
    let i2c = embassy_rp::i2c::I2c::new_blocking(p.I2C0, scl, sda, config);

    let alert_pin = Input::new(p.PIN_0, Pull::Up);
    let adc = Ads1x1x::new_ads1115(i2c, TargetAddr::default());
    debug!("i2c bus created");

    spawner.must_spawn(slider1_control(adc));
    spawner.must_spawn(slider1_interupt(alert_pin));
}

#[embassy_executor::task]
async fn slider1_interupt(mut alert_pin: Input<'static>) {
    loop {
        alert_pin.wait_for_low().await;
        debug!("Slider event detected.");
        SLIDER1_EVENT.signal(Slider::ReadSlider);
        embassy_time::Delay.delay_ms(10).await;
    }
}

#[embassy_executor::task]
async fn slider1_control(mut adc: Adc<'static, ads1x1x::mode::OneShot>) {
    adc.set_full_scale_range(ads1x1x::FullScaleRange::Within2_048V)
        .unwrap();

    adc.set_data_rate(ads1x1x::DataRate16Bit::Sps8).unwrap();

    adc.set_comparator_polarity(ads1x1x::ComparatorPolarity::ActiveLow)
        .unwrap();

    adc.set_comparator_queue(ads1x1x::ComparatorQueue::Four)
        .unwrap();

    let baseline = block!(adc.read(channel::SingleA0)).unwrap();

    info!("Initial vlaue {}", baseline);

    let low_threshold = baseline.saturating_sub(THRESHOLD);
    let high_threshold = baseline.saturating_add(THRESHOLD);
    debug!("Setup initial low_threshold set to: {}", low_threshold);
    debug!("Setup initial high_threshold set to: {}", high_threshold);

    adc.set_low_threshold_raw(low_threshold).unwrap();
    adc.set_high_threshold_raw(high_threshold).unwrap();
    adc.set_comparator_latching(ComparatorLatching::Nonlatching)
        .unwrap();

    adc.set_comparator_mode(ads1x1x::ComparatorMode::Window)
        .unwrap();
    match adc.into_continuous() {
        Err(_) => panic!("Failed to go into continuous mode."),
        Ok(mut adc) => loop {
            let event = SLIDER1_EVENT.wait().await;
            match event {
                Slider::ReadSlider => {
                    let value = adc.read().unwrap();
                    let low_threshold = value.saturating_sub(THRESHOLD);
                    let high_threshold = value.saturating_add(THRESHOLD);
                    debug!("Setup initial low_threshold set to: {}", low_threshold);
                    debug!("Setup initial high_threshold set to: {}", high_threshold);
                    adc.set_low_threshold_raw(value.saturating_sub(THRESHOLD))
                        .unwrap();
                    adc.set_high_threshold_raw(value.saturating_add(THRESHOLD))
                        .unwrap();
                    let percentage = value as f64 / 32767.0 * 100.0;
                    let int_percentage = round(percentage) as i8;
                    info!("Current value for adc: {}", value);
                    info!("Current volume percentage: {}", int_percentage);
                } //               Slider::MoveSlider => panic!("Moving the slider is not yet implemented."),
            };
            debug!("Detected Potentiometer value change.");
        },
    }
}
