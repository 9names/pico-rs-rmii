use crate::delay::Delay;
use embedded_hal::digital::v2::{InputPin, OutputPin, PinState};
use rp2040_hal::gpio::DynPin;

pub struct Mdio {
    md_io: DynPin,
    md_ck: DynPin,
    delay: crate::delay::Delay,
}

impl Mdio {
    pub fn new(md_io: DynPin, md_ck: DynPin) -> Mdio {
        Mdio {
            md_io,
            md_ck,
            delay: Delay::new(),
        }
    }

    pub fn read(&mut self, addr: u8, reg: u16) -> u16 {
        self.md_ck.into_push_pull_output();
        self.md_io.into_push_pull_output();

        // Clear the state machine by clocking out 32 bits
        for _ in 0..32 {
            self.bit_clock_out(1);
        }
        // ST
        self.bit_clock_out(0);
        self.bit_clock_out(1);

        // OP (read)
        self.bit_clock_out(1);
        self.bit_clock_out(0);

        // PA5
        for offset in (0..5u8).rev() {
            let bit = (addr >> offset) & 0x01;
            self.bit_clock_out(bit);
        }

        // RA5
        for offset in (0..5u8).rev() {
            let bit = (reg >> offset) & 0x01;
            self.bit_clock_out(bit as u8);
        }

        // TA
        self.bit_clock_out(0);
        self.bit_clock_out(0);
        self.md_io.into_floating_input();

        let mut data: u16 = 0;
        for _ in 0..16 {
            data <<= 1;
            data |= self.bit_clock_in();
        }
        crate::debug!("mdio read {:X}", data);
        data
    }

    pub fn write(&mut self, addr: u8, reg: u16, value: u16) {
        self.md_ck.into_push_pull_output();
        self.md_io.into_push_pull_output();

        // Clear the state machine by clocking out 32 bits
        for _ in 0..32 {
            self.bit_clock_out(1);
        }
        // ST
        self.bit_clock_out(0);
        self.bit_clock_out(1);

        // OP (write)
        self.bit_clock_out(0);
        self.bit_clock_out(1);

        // PA5
        for offset in (0..5u8).rev() {
            let bit = (addr >> offset) & 0x01;
            self.bit_clock_out(bit);
        }

        // RA5
        for offset in (0..5u8).rev() {
            let bit = (reg >> offset) & 0x01;
            self.bit_clock_out(bit as u8);
        }

        // TA
        self.bit_clock_out(1);
        self.bit_clock_out(0);

        for offset in (0..16u8).rev() {
            let bit = (value >> offset) & 0x01;
            self.bit_clock_out(bit as u8);
        }
        crate::debug!("mdio write {:X}", value);
        self.md_io.into_floating_input();
    }

    fn bit_clock_out(&mut self, bit: u8) {
        self.md_ck.set_low().unwrap();
        self.delay.delay_us(1);
        let pinstate = if bit == 1 {
            PinState::High
        } else {
            PinState::Low
        };
        self.md_io.set_state(pinstate).unwrap();
        self.md_ck.set_high().unwrap();
        self.delay.delay_us(1);
    }

    fn bit_clock_in(&mut self) -> u16 {
        self.md_ck.set_low().unwrap();
        self.delay.delay_us(1);
        self.md_ck.set_high().unwrap();
        self.md_io.into_floating_input();
        let bit = if self.md_io.is_high().unwrap() { 1 } else { 0 };
        self.delay.delay_us(1);
        bit
    }
}
