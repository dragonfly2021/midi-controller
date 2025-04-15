#![no_std]
pub mod slider {
    use ads1x1x::Ads1x1x;
    use defmt::debug;
    use embassy_rp::{
        i2c::{self, Blocking},
        peripherals::I2C0,
    };
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::signal::Signal;
    use libm::round;
    use nb::block;

    pub type Adc<'a> = Ads1x1x<
        i2c::I2c<'static, I2C0, Blocking>,
        ads1x1x::ic::Ads1115,
        ads1x1x::ic::Resolution16Bit,
        ads1x1x::mode::OneShot,
    >;

    pub type SliderActionType = &'static Signal<CriticalSectionRawMutex, SliderAction>;

    pub type SliderValueUpdateType = &'static Signal<CriticalSectionRawMutex, SliderValue>;

    pub enum Error {
        FailedUpdate,
    }

    pub struct Slider {
        adc: Option<Adc<'static>>,
        threshold: i16,
        action_event: SliderActionType,
        slider_value_event: SliderValueUpdateType,
        slider_percent: i8,
        observed_max: i16,
        observed_min: i16,
    }

    pub enum SliderValue {
        Percent(i8),
    }
    pub enum SliderAction {
        MoveSlider(i8),
        ReadSlider,
    }

    impl<'a> Slider {
        pub fn new(
            mut adc: Adc<'a>,
            threshold: i16,
            action_event: SliderActionType,
            slider_value_event: SliderValueUpdateType,
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

            let observed_max = 26150;
            let observed_min = 10;

            let slider_percent = 0;
            Self {
                adc: Some(adc),
                threshold,
                action_event,
                slider_value_event,
                slider_percent,
                observed_max,
                observed_min,
            }
        }

        pub fn baseline(&mut self) {
            let mut baseline_adc = self
                .adc
                .take()
                .expect("Failed to take the ADC to perform the baseline.");
            let baseline = block!(baseline_adc.read(ads1x1x::channel::SingleA0))
                .expect("Failed to read value");
            self.adc = Some(baseline_adc);
            debug!("Inial value for slider {}", baseline);
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

        fn set_thresholds(&mut self, low: i16, high: i16) {
            let mut threshold_adc = self
                .adc
                .take()
                .expect("Unable to take the adc to set the threshold.");
            threshold_adc.set_low_threshold_raw(low).unwrap();
            threshold_adc.set_high_threshold_raw(high).unwrap();
            self.adc = Some(threshold_adc);
        }

        pub fn value_to_percent(&self, value: i16) -> i8 {
            round(value as f64 / (self.observed_max - self.observed_min) as f64 * 100.0) as i8
        }

        pub fn percent_to_value(&self, percent: i8) -> i16 {
            (percent as f64 / 100.0 * (self.observed_max - self.observed_min) as f64
                + ((self.observed_max - self.observed_min) as f64 / 50.0)) as i16
        }

        pub async fn into_continuous(&mut self) {
            let continuous_adc = self.adc.take().expect("ADC already taken");

            match continuous_adc.into_continuous() {
                Err(_) => panic!("Failed to go into continuous read mode."),
                Ok(adc) => {
                    let event = self.action_event.wait().await;
                    match event {
                        SliderAction::ReadSlider => match adc.into_one_shot() {
                            Err(_) => panic!("Failed to return to one shot mode."),
                            Ok(a) => {
                                self.adc = Some(a);
                                self.baseline();
                            }
                        },
                        SliderAction::MoveSlider(_val) => {
                            todo!()
                        }
                    }
                }
            }
        }
    }
}
