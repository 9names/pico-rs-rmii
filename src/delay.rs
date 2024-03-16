use embedded_hal::blocking::delay::{DelayMs, DelayUs};
use rp2040_hal::pac;

pub struct Delay {
    timer: pac::TIMER,
}

impl Delay {
    pub fn new() -> Delay {
        let timer = unsafe { pac::Peripherals::steal().TIMER };

        Delay { timer }
    }
    fn get_us(&mut self) -> u32 {
        self.timer.timerawl().read().bits()
    }

    pub fn delay_us(&mut self, us: u32) {
        let start = self.get_us();
        while (self.get_us().wrapping_sub(start) as i32) < us as i32 {}
    }

    pub fn delay_ms(&mut self, ms: u32) {
        self.delay_us(ms * 1000)
    }
}

impl DelayUs<u32> for Delay {
    fn delay_us(&mut self, us: u32) {
        self.delay_us(us)
    }
}

impl DelayMs<u32> for Delay {
    fn delay_ms(&mut self, ms: u32) {
        self.delay_ms(ms)
    }
}
