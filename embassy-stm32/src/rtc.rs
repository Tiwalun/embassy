use crate::pac::PWR;
use crate::pac::RCC;
use crate::pac::RTC as PAC_RTC;
use crate::peripherals::RTC;
use crate::rcc::sealed::RccPeripheral;
use crate::rcc::RtcClkSrc;
use embassy::util::Unborrow;
use embassy_hal_common::unborrow;

const RTC_CLK_DIV: u8 = 16;
const ASYNCH_PREDIV: u8 = RTC_CLK_DIV - 1;
const SYNCH_PREDIV: u16 = 0x7FFF;

pub struct Rtc {
    peripheral: crate::peripherals::RTC,
}

impl Rtc {
    pub fn new(peripheral: impl Unborrow<Target = RTC>, rtc_src: RtcClkSrc) -> Self {
        unborrow!(peripheral);

        // Write twice to ensure write is not cached (according to ST)
        unsafe {
            PWR.cr1().modify(|r| r.set_dbp(true));
            PWR.cr1().modify(|r| r.set_dbp(true));
        }

        unsafe {
            RCC.apb1enr1().modify(|r| r.set_rtcapben(true));

            RCC.bdcr().modify(|r| r.set_rtcsel(rtc_src as u8));

            RCC.bdcr().modify(|r| r.set_rtcen(true));
        }

        unsafe {
            write_protection(&mut peripheral, false);
            {
                init_mode(&mut peripheral, true);
                {
                    PAC_RTC.cr().modify(|r| {
                        r.set_fmt(false);

                        r.set_osel(0b11);
                        /*
                            00: Output disabled
                            01: Alarm A output enabled
                            10: Alarm B output enabled
                            11: Wakeup output enabled
                        */
                        r.set_pol(false);
                    });

                    PAC_RTC.cr().modify(|w| w.set_wucksel(0b000));

                    PAC_RTC.prer().modify(|w| {
                        w.set_prediv_s(SYNCH_PREDIV);

                        w.set_prediv_a(ASYNCH_PREDIV);
                    });
                }
                init_mode(&mut peripheral, false);

                PAC_RTC.or().modify(|w| {
                    w.set_rtc_alarm_type(false);
                    w.set_rtc_out_rmp(false);
                });
            }
            write_protection(&mut peripheral, true);
        }

        Self { peripheral }
    }
}

unsafe fn write_protection(_rtc: &mut RTC, enable: bool) {
    if enable {
        PAC_RTC.wpr().write(|w| w.set_key(0xFF));
    } else {
        PAC_RTC.wpr().write(|w| w.set_key(0xCA));
        PAC_RTC.wpr().write(|w| w.set_key(0x53));
    }
}

unsafe fn init_mode(_rtc: &mut RTC, enabled: bool) {
    use crate::pac::rtc::regs::Isr;

    if enabled {
        let isr = PAC_RTC.isr().read();

        if !isr.initf() {
            PAC_RTC.isr().write_value(Isr(0xFFFFFFFF)); // Sets init mode
            while !PAC_RTC.isr().read().initf() {} // wait to return to init state
        }
    } else {
        PAC_RTC.isr().write(|w| w.set_init(false)); // Exits init mode
    }
}
