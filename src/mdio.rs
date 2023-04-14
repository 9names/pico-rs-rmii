use embedded_hal::blocking::delay::DelayUs;
use embedded_hal::digital::v2::{InputPin, OutputPin, PinState};
use rp2040_hal::gpio::DynPin;

pub struct Uninitialized;
pub struct Initialized;

//    timer: &'a mut dyn embedded_hal::timer::CountDown<Time = u32>,

pub struct Mdio<State> {
    md_io: DynPin,
    md_ck: DynPin,
    _phantom: core::marker::PhantomData<State>,
}

impl Mdio<Uninitialized> {
    pub fn init(md_io: DynPin, md_ck: DynPin) -> Mdio<Initialized> {
        Mdio {
            md_io,
            md_ck,
            _phantom: core::marker::PhantomData,
        }
    }
}
impl Mdio<Initialized> {
    pub fn read(&mut self, addr: u8, reg: u16, delay: &mut dyn DelayUs<u16>) -> u16 {
        self.md_ck.into_push_pull_output();
        self.md_io.into_push_pull_output();

        // Clear the state machine by clocking out 32 bits
        for _ in 0..32 {
            self.bit_clock_out(delay, 1);
        }
        // ST
        self.bit_clock_out(delay, 0);
        self.bit_clock_out(delay, 1);

        // OP (read)
        self.bit_clock_out(delay, 1);
        self.bit_clock_out(delay, 0);

        // PA5
        for offset in 0..5 {
            let bit = (addr >> (4 - offset)) & 0x01;
            self.bit_clock_out(delay, bit as u8);
        }

        // RA5
        for offset in 0..5 {
            let bit = (reg >> (4 - offset)) & 0x01;
            self.bit_clock_out(delay, bit as u8);
        }

        // TA
        self.bit_clock_out(delay, 0);
        self.bit_clock_out(delay, 0);
        self.md_io.into_floating_input();

        let mut data: u16 = 0;
        for _ in 0..16 {
            data <<= 1;
            data |= self.bit_clock_in(delay);
        }
        crate::debug!("mdio read {:X}", data);
        data
    }

    pub fn write(&mut self, addr: u8, reg: u16, value: u16, delay: &mut dyn DelayUs<u16>) {
        self.md_ck.into_push_pull_output();
        self.md_io.into_push_pull_output();

        // Clear the state machine by clocking out 32 bits
        for _ in 0..32 {
            self.bit_clock_out(delay, 1);
        }
        // ST
        self.bit_clock_out(delay, 0);
        self.bit_clock_out(delay, 1);

        // OP (write)
        self.bit_clock_out(delay, 0);
        self.bit_clock_out(delay, 1);

        // PA5
        for offset in 0..5 {
            let bit = (addr >> (4 - offset)) & 0x01;
            self.bit_clock_out(delay, bit as u8);
        }

        // RA5
        for offset in 0..5 {
            let bit = (reg >> (4 - offset)) & 0x01;
            self.bit_clock_out(delay, bit as u8);
        }

        // TA
        self.bit_clock_out(delay, 1);
        self.bit_clock_out(delay, 0);

        for offset in 0..16 {
            let bit = (value >> (15 - offset)) & 0x01;
            self.bit_clock_out(delay, bit as u8);
        }
        crate::debug!("mdio write {:X}", value);
        self.md_io.into_floating_input();
    }

    fn bit_clock_out(&mut self, delay: &mut dyn DelayUs<u16>, bit: u8) {
        self.md_ck.set_low().unwrap();
        delay.delay_us(1);
        let pinstate = if bit == 1 {
            PinState::High
        } else {
            PinState::Low
        };
        self.md_io.set_state(pinstate).unwrap();
        self.md_ck.set_high().unwrap();
        delay.delay_us(1);
    }

    fn bit_clock_in(&mut self, delay: &mut dyn DelayUs<u16>) -> u16 {
        self.md_ck.set_low().unwrap();
        delay.delay_us(1);
        self.md_ck.set_high().unwrap();
        self.md_io.into_floating_input();
        let bit = if self.md_io.is_high().unwrap() { 1 } else { 0 };
        delay.delay_us(1);
        bit
    }
}
