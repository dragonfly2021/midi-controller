#![no_std]
#![no_main]

//use defmt::info;
use ads1x1x::{Ads1x1x, ComparatorLatching, TargetAddr, channel};
use defmt::{debug, info};
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::i2c::InterruptHandler;
use libm::round;
use nb::block;
use {defmt_rtt as _, panic_probe as _};

embassy_rp::bind_interrupts!(struct Irqs {
    I2C0_IRQ => InterruptHandler<embassy_rp::peripherals::I2C0>;
    I2C1_IRQ => InterruptHandler<embassy_rp::peripherals::I2C1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    debug!("Performing startup.");

    let p = embassy_rp::init(Default::default());

    let sda = p.PIN_20;
    let scl = p.PIN_21;
    let config = embassy_rp::i2c::Config::default();
    let i2c = embassy_rp::i2c::I2c::new_blocking(p.I2C0, scl, sda, config);

    let mut alert_pin = Input::new(p.PIN_0, Pull::Up);
    let mut adc = Ads1x1x::new_ads1115(i2c, TargetAddr::default());
    debug!("i2c bus created");

    let threshold = 20;
    adc.set_full_scale_range(ads1x1x::FullScaleRange::Within2_048V)
        .unwrap();

    adc.set_data_rate(ads1x1x::DataRate16Bit::Sps860).unwrap();

    adc.set_comparator_polarity(ads1x1x::ComparatorPolarity::ActiveLow)
        .unwrap();

    adc.set_comparator_queue(ads1x1x::ComparatorQueue::Four)
        .unwrap();

    let baseline = block!(adc.read(channel::SingleA0)).unwrap();

    info!("Initial vlaue {}", baseline);

    let mut low_threshold = baseline.saturating_sub(threshold);
    let mut high_threshold = baseline.saturating_add(threshold);
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
            alert_pin.wait_for_low().await;
            debug!("Detected Potentiometer value change.");

            let value = adc.read().unwrap();
            low_threshold = value.saturating_sub(threshold);
            high_threshold = value.saturating_add(threshold);
            debug!("Setup initial low_threshold set to: {}", low_threshold);
            debug!("Setup initial high_threshold set to: {}", high_threshold);
            adc.set_low_threshold_raw(value.saturating_sub(threshold))
                .unwrap();
            adc.set_high_threshold_raw(value.saturating_add(threshold))
                .unwrap();
            let percentage = value as f64 / 32767.0 * 100.0;
            let int_percentage = round(percentage) as i8;
            info!("Current value for adc: {}", value);
            info!("Current volume percentage: {}", int_percentage);
        },
    }
}
