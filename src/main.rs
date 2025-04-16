#![no_std]
#![no_main]

mod dcmotor;
mod slider;

//use defmt::info;
use ads1x1x::{Ads1x1x, TargetAddr};
use dcmotor::DcMotor;
use defmt::debug;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::pwm::Pwm;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use embedded_hal_async::delay::DelayNs;
use slider::{Slider, SliderAction, SliderActionType, SliderValue};

use {defmt_rtt as _, panic_probe as _};

const THRESHOLD: i16 = 20;

static SLIDER1_SIGNAL: Signal<CriticalSectionRawMutex, SliderAction> = Signal::new();
static SLIDER1_UPDATE: Signal<CriticalSectionRawMutex, SliderValue> = Signal::new();

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    debug!("Performing startup.");

    let p = embassy_rp::init(Default::default());

    // Setting up fader 1;
    let sda = p.PIN_20;
    let scl = p.PIN_21;
    let config = embassy_rp::i2c::Config::default();
    let i2c = embassy_rp::i2c::I2c::new_blocking(p.I2C0, scl, sda, config);

    let alert_pin = Input::new(p.PIN_0, Pull::Up);
    let adc = Ads1x1x::new_ads1115(i2c, TargetAddr::default());
    let ain1 = embassy_rp::gpio::Output::new(p.PIN_17, embassy_rp::gpio::Level::Low);
    let ain2 = embassy_rp::gpio::Output::new(p.PIN_18, embassy_rp::gpio::Level::Low);
    let pwm = Pwm::new_output_a(p.PWM_SLICE0, p.PIN_16, Default::default());
    let motor1 = DcMotor::new(ain1, ain2, pwm);

    spawner.must_spawn(slider_interupt(alert_pin, &SLIDER1_SIGNAL));
    spawner.must_spawn(slider_control(
        adc,
        &SLIDER1_SIGNAL,
        &SLIDER1_UPDATE,
        motor1,
    ));

    spawner.must_spawn(idle_monitor());
    spawner.must_spawn(move_fader());
}

#[embassy_executor::task]
async fn slider_interupt(mut alert_pin: Input<'static>, slider_event: SliderActionType) {
    loop {
        alert_pin.wait_for_low().await;
        debug!("Slider event detected.");
        slider_event.signal(SliderAction::ReadSlider);
        embassy_time::Delay.delay_ms(10).await;
    }
}

#[embassy_executor::task]
async fn slider_control(
    adc: slider::Adc<'static>,
    action_event: slider::SliderActionType,
    update_event: slider::SliderValueUpdateType,
    motor: DcMotor<'static>,
) {
    let mut control = Slider::new(adc, THRESHOLD, action_event, update_event, motor);
    control.baseline();
    loop {
        control.into_continuous().await;
    }
}

#[embassy_executor::task]
async fn idle_monitor() {
    let mut idle_time = Duration::from_millis(0);
    let mut last_report = Instant::now();

    loop {
        let start = Instant::now();

        // Wait a bit to simulate "idle time"
        Timer::after(Duration::from_micros(100)).await;

        let elapsed = Instant::now() - start;
        idle_time += elapsed;

        if Instant::now() - last_report >= Duration::from_secs(1) {
            let idle_pct = (idle_time.as_millis() * 100) / 1000;
            defmt::info!("CPU Idle: {}%", idle_pct);
            idle_time = Duration::from_millis(0);
            last_report = Instant::now();
        }
    }
}

#[embassy_executor::task]
async fn move_fader() {
    let numbers = [26, 53, 17, 76, 23, 89, 52, 100, 0];
    loop {
        for num in numbers {
            Timer::after(Duration::from_secs(1)).await;
            SLIDER1_SIGNAL.signal(SliderAction::MoveSlider(num));
            debug!("Sending the slider to {}%", num);
        }
    }
}
