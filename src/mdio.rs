use crate::delay::Delay;
// use embedded_hal_0_2::digital::v2::{InputPin, OutputPin, PinState};
use embedded_hal::digital::{InputPin, OutputPin, PinState};
use ieee802_3_miim::Miim;
use rp2040_hal::gpio::{
    bank0::Gpio14, DynFunction, DynPinId, DynSioConfig, FunctionSio, FunctionSioOutput, InOutPin,
    Pin, PullDown, PullNone, SioOutput,
};

impl Miim for Mdio {
    fn read(&mut self, phy: u8, reg: u8) -> u16 {
        self.read_reg(phy, reg)
    }

    fn write(&mut self, phy: u8, reg: u8, data: u16) {
        self.write_reg(phy, reg, data)
    }
}

pub struct Mdio {
    md_io: Pin<DynPinId, DynFunction, PullDown>,
    md_ck: Pin<DynPinId, FunctionSioOutput, PullNone>,
    delay: crate::delay::Delay,
}

impl Mdio {
    pub fn new(
        md_io: Pin<DynPinId, DynFunction, PullDown>,
        md_ck: Pin<DynPinId, FunctionSioOutput, PullNone>,
    ) -> Mdio {
        Mdio {
            md_io,
            md_ck,
            delay: Delay::new(),
        }
    }

    pub fn read_reg(&mut self, addr: u8, reg: u8) -> u16 {
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
            self.bit_clock_out(bit);
        }

        // TA
        self.bit_clock_out(0);
        self.bit_clock_out(0);

        let mut data: u16 = 0;
        for _ in 0..16 {
            data <<= 1;
            data |= self.bit_clock_in();
        }
        crate::trace!("mdio read {:X}", data);
        data
    }

    pub fn write_reg(&mut self, addr: u8, reg: u8, value: u16) {
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
            self.bit_clock_out(bit);
        }

        // TA
        self.bit_clock_out(1);
        self.bit_clock_out(0);

        for offset in (0..16u8).rev() {
            let bit = (value >> offset) & 0x01;
            self.bit_clock_out(bit as u8);
        }
        crate::trace!("mdio write {:X}", value);
    }

    fn bit_clock_out(&mut self, bit: u8) {
        // set i/o pin as push-pull output
        self.md_io
            .try_set_function(DynFunction::Sio(DynSioConfig::Output))
            .unwrap();
        self.md_ck.set_low().unwrap();
        let pinstate = if bit == 1 {
            PinState::High
        } else {
            PinState::Low
        };
        self.delay.delay_us(1);
        self.md_io.set_state(pinstate).unwrap();
        self.md_ck.set_high().unwrap();
    }

    fn bit_clock_in(&mut self) -> u16 {
        // set i/o pin as floating input
        self.md_io
            .try_set_function(DynFunction::Sio(DynSioConfig::Input))
            .unwrap();
        self.md_ck.set_low().unwrap();
        self.delay.delay_us(1);
        let bit = if self.md_io.is_high().unwrap() { 1 } else { 0 };
        self.md_ck.set_high().unwrap();
        bit
    }
}
