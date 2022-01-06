use crate::pac::PWR;
use crate::peripherals;

pub struct Power {
    peripheral: peripherals::PWR,
}

impl Power {
    pub fn new(peri: peripherals::PWR) -> Self {
        Self { peripheral: peri }
    }

    pub fn boot_cpu2(&mut self) {
        unsafe { PWR.cr4().modify(|r| r.set_c2boot(true)) }
    }
}
