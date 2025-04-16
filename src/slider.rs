use crate::dcmotor::DcMotor;

use ads1x1x::{Ads1x1x, channel};
use defmt::debug;
use embassy_rp::{
    i2c::{self, Blocking},
    peripherals::I2C0,
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use libm::{fabs, round};
use nb::block;

pub type Adc<'a> = Ads1x1x<
    i2c::I2c<'static, I2C0, Blocking>,
    ads1x1x::ic::Ads1115,
    ads1x1x::ic::Resolution16Bit,
    ads1x1x::mode::OneShot,
>;

pub type SliderActionType = &'static Signal<CriticalSectionRawMutex, SliderAction>;

pub type SliderValueUpdateType = &'static Signal<CriticalSectionRawMutex, SliderValue>;

pub struct Slider<'a> {
    adc: Option<Adc<'a>>,
    threshold: i16,
    action_event: SliderActionType,
    slider_value_event: SliderValueUpdateType,
    slider_percent: i8,
    observed_max: i16,
    observed_min: i16,
    motor: DcMotor<'a>,
}

pub enum SliderValue {
    Percent(i8),
}
pub enum SliderAction {
    MoveSlider(i8),
    ReadSlider,
}

impl<'a> Slider<'a> {
    pub fn new(
        mut adc: Adc<'a>,
        threshold: i16,
        action_event: SliderActionType,
        slider_value_event: SliderValueUpdateType,
        motor: DcMotor<'a>,
    ) -> Self {
        adc.set_full_scale_range(ads1x1x::FullScaleRange::Within4_096V)
            .unwrap();
        adc.set_data_rate(ads1x1x::DataRate16Bit::Sps250).unwrap();
        adc.set_comparator_polarity(ads1x1x::ComparatorPolarity::ActiveLow)
            .unwrap();

        adc.set_comparator_queue(ads1x1x::ComparatorQueue::Four)
            .unwrap();

        adc.set_comparator_mode(ads1x1x::ComparatorMode::Window)
            .unwrap();

        let observed_max = 25000;
        let observed_min = 350;

        let slider_percent = 0;
        Self {
            adc: Some(adc),
            threshold,
            action_event,
            slider_value_event,
            slider_percent,
            observed_max,
            observed_min,
            motor,
        }
    }

    pub fn baseline(&mut self) {
        let mut baseline_adc = self
            .adc
            .take()
            .expect("Failed to take the ADC to perform the baseline.");
        let baseline =
            block!(baseline_adc.read(ads1x1x::channel::SingleA0)).expect("Failed to read value");
        self.adc = Some(baseline_adc);
        self.set_thresholds(
            baseline.saturating_sub(self.threshold),
            baseline.saturating_add(self.threshold),
        );

        if self.observed_min > baseline {
            self.observed_min = baseline;
        }

        if self.observed_max < baseline {
            self.observed_max = baseline;
        }

        let new_slider_percent = self.value_to_percent(baseline);
        if self.slider_percent != new_slider_percent {
            self.slider_value_event
                .signal(SliderValue::Percent(new_slider_percent));
            self.slider_percent = new_slider_percent;
            debug!("Value for slider changed to: {}", new_slider_percent);
        }
    }

    pub fn current_value(&mut self) -> i16 {
        let mut current_value_adc = self.adc.take().expect("Failed to get current value adc.");
        let read = block!(current_value_adc.read(channel::SingleA0)).unwrap();
        self.adc = Some(current_value_adc);
        read
    }

    fn observed_diff(&mut self) -> i16 {
        self.observed_max - self.observed_min
    }

    fn set_thresholds(&mut self, low: i16, high: i16) {
        let mut threshold_adc = self
            .adc
            .take()
            .expect("Unable to take the adc to set the threshold.");
        threshold_adc.set_low_threshold_raw(low).unwrap();
        threshold_adc.set_high_threshold_raw(high).unwrap();
        self.adc = Some(threshold_adc);
    }

    fn value_to_percent(&mut self, value: i16) -> i8 {
        round((value - self.observed_min) as f64 / (self.observed_diff()) as f64 * 100.0) as i8
    }

    fn percent_to_value(&mut self, percent: i8) -> i16 {
        (percent as f64 / 100.0 * self.observed_diff() as f64
            + (self.observed_diff() as f64 / 200.0)
            + self.observed_min as f64) as i16
    }

    fn calculate_speed(&mut self, distance: i16) -> u8 {
        let speed: u8 = match distance {
            x if x > 10000 => 90,
            x if x > 7000 => 85,
            x if x > 5000 => 80,
            _ => 75,
        };
        speed
    }

    async fn goto_location(&mut self, location: i8) {
        let target = self.percent_to_value(location);
        let mut previous_distance = 32767;
        loop {
            let current_loc = self.current_value();

            let distance = fabs(target as f64 - current_loc as f64) as i16;
            //debug!(
            //    "We need to move: {}, current: {}, target: {}",
            //    distance, current_loc, target
            //);
            if previous_distance < distance {
                self.motor.stop();
                panic!("We are going the wrong way!");
            }

            if distance < 500 {
                self.motor.stop();
                break;
            }
            if target > current_loc.saturating_sub(self.threshold) {
                let speed = self.calculate_speed(distance);
                //debug!(
                //    "Moving forward from {} to {} at speed: {}",
                //    current_loc, target, speed
                //);
                self.motor.forward(speed);
            }
            if target < current_loc.saturating_add(self.threshold) {
                let speed = self.calculate_speed(distance);
                //debug!(
                //    "Moving backwards from {} to {} at speed: {}",
                //    current_loc, target, speed
                //);
                self.motor.reverse(speed);
            }
            if distance.saturating_sub(self.threshold) > previous_distance {
                previous_distance = distance;
            }
            Timer::after(Duration::from_millis(1)).await;
        }
    }

    pub async fn into_continuous(&mut self) {
        let continuous_adc = self.adc.take().expect("ADC already taken");

        match continuous_adc.into_continuous() {
            Err(_) => panic!("Failed to go into continuous read mode."),
            Ok(adc) => {
                let event = self.action_event.wait().await;
                match adc.into_one_shot() {
                    Err(_) => panic!("Failed to return to one shot mode."),
                    Ok(a) => {
                        self.adc = Some(a);
                    }
                }

                match event {
                    SliderAction::ReadSlider => {
                        self.baseline();
                    }
                    SliderAction::MoveSlider(val) => {
                        self.goto_location(val).await;
                        self.baseline();
                    }
                }
            }
        }
    }
}
