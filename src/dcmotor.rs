use embassy_rp::gpio::Output;
use embassy_rp::pwm::{Pwm, SetDutyCycle};

pub struct DcMotor<'a> {
    ain1: Output<'a>,
    ain2: Output<'a>,
    pwm: Pwm<'a>,
}

impl<'a> DcMotor<'a> {
    pub fn new(ain1: Output<'a>, ain2: Output<'a>, pwm: Pwm<'a>) -> Self {
        Self { ain1, ain2, pwm }
    }

    pub fn forward(&mut self, speed: u8) {
        let _ = self.ain1.set_high();
        let _ = self.ain2.set_low();
        self.pwm.set_duty_cycle_percent(speed).unwrap();
    }

    pub fn reverse(&mut self, speed: u8) {
        let _ = self.ain1.set_low();
        let _ = self.ain2.set_high();
        self.pwm.set_duty_cycle_percent(speed).unwrap();
    }

    pub fn stop(&mut self) {
        let _ = self.ain1.set_low();
        let _ = self.ain2.set_low();
        self.pwm.set_duty_cycle_percent(0).unwrap();
    }
}
