#![no_std]
#![no_main]

//use defmt::info;
use ads1x1x::{Ads1x1x, TargetAddr};
use defmt::debug;
use embassy_rp::gpio::{Input, Pull};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embedded_hal_async::delay::DelayNs;
use midi_controller::slider::{self, Slider, SliderAction, SliderActionType, SliderValue};
use {defmt_rtt as _, panic_probe as _};

const THRESHOLD: i16 = 20;

static SLIDER1_SIGNAL: Signal<CriticalSectionRawMutex, SliderAction> = Signal::new();
static SLIDER1_UPDATE: Signal<CriticalSectionRawMutex, SliderValue> = Signal::new();

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    debug!("Performing startup.");

    let p = embassy_rp::init(Default::default());

    let sda = p.PIN_20;
    let scl = p.PIN_21;
    let config = embassy_rp::i2c::Config::default();
    let i2c = embassy_rp::i2c::I2c::new_blocking(p.I2C0, scl, sda, config);

    let alert_pin = Input::new(p.PIN_0, Pull::Up);
    let adc = Ads1x1x::new_ads1115(i2c, TargetAddr::default());
    spawner.must_spawn(slider_interupt(alert_pin, &SLIDER1_SIGNAL));
    spawner.must_spawn(slider_control(adc, &SLIDER1_SIGNAL, &SLIDER1_UPDATE));
}

#[embassy_executor::task]
async fn slider_interupt(mut alert_pin: Input<'static>, slider_event: SliderActionType) {
    loop {
        alert_pin.wait_for_low().await;
        debug!("Slider event detected.");
        slider_event.signal(SliderAction::ReadSlider);
        embassy_time::Delay.delay_ms(50).await;
    }
}

#[embassy_executor::task]
async fn slider_control(
    adc: slider::Adc<'static>,
    action_event: slider::SliderActionType,
    update_event: slider::SliderValueUpdateType,
) {
    let mut control = Slider::new(adc, THRESHOLD, action_event, update_event);
    control.baseline();
    loop {
        control.into_continuous().await;
    }
}
