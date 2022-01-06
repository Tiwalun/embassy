use crate::pac;
use crate::pac::{FLASH, PWR};
use crate::peripherals::{self, RCC};
use crate::rcc::{get_freqs, set_freqs, Clocks};
use crate::time::Hertz;
use crate::time::U32Ext;
use core::marker::PhantomData;
use embassy::util::Unborrow;
use embassy_hal_common::unborrow;

/// Most of clock setup is copied from stm32l0xx-hal, and adopted to the generated PAC,
/// and with the addition of the init function to configure a system clock.

/// Only the basic setup using the HSE and HSI clocks are supported as of now.

/// HSI speed
pub const HSI_FREQ: u32 = 16_000_000;

/// HSE speed
///
/// Fixed to 32 MHz on WB55.
pub const HSE_FREQ: u32 = 32_000_000;

/// System clock mux source
#[derive(Clone, Copy)]
pub enum ClockSrc {
    HSE(HseDivider),
    HSI16,
    Pll(PllSrc),
    Msi,
}

#[derive(Debug, Clone, Copy)]
pub enum MsiRange {
    #[doc = "range 0 around 100 kHz"]
    RANGE100K = 0,
    #[doc = "range 1 around 200 kHz"]
    RANGE200K = 1,
    #[doc = "range 2 around 400 kHz"]
    RANGE400K = 2,
    #[doc = "range 3 around 800 kHz"]
    RANGE800K = 3,
    #[doc = "range 4 around 1 MHz"]
    RANGE1M = 4,
    #[doc = "range 5 around 2 MHz"]
    RANGE2M = 5,
    #[doc = "range 6 around 4 MHz"]
    RANGE4M = 6,
    #[doc = "range 7 around 8 MHz"]
    RANGE8M = 7,
    #[doc = "range 8 around 16 MHz"]
    RANGE16M = 8,
    #[doc = "range 9 around 24 MHz"]
    RANGE24M = 9,
    #[doc = "range 10 around 32 MHz"]
    RANGE32M = 10,
    #[doc = "range 11 around 48 MHz"]
    RANGE48M = 11,
}

/// HSE input divider.
#[derive(Debug, Clone, Copy)]
pub enum HseDivider {
    NotDivided,
    Div2,
}

#[derive(Debug, Clone, Copy)]
pub enum PllSrc {
    Msi(MsiRange),
    Hsi,
    Hse(HseDivider),
}

/// AHB prescaler
#[derive(Clone, Copy, PartialEq)]
pub enum AHBPrescaler {
    NotDivided,
    Div2,
    Div3,
    Div4,
    Div5,
    Div6,
    Div8,
    Div10,
    Div16,
    Div32,
    Div64,
    Div128,
    Div256,
    Div512,
}

/// APB prescaler
#[derive(Clone, Copy)]
pub enum APBPrescaler {
    NotDivided,
    Div2,
    Div4,
    Div8,
    Div16,
}

impl Into<u8> for APBPrescaler {
    fn into(self) -> u8 {
        match self {
            APBPrescaler::NotDivided => 1,
            APBPrescaler::Div2 => 0x04,
            APBPrescaler::Div4 => 0x05,
            APBPrescaler::Div8 => 0x06,
            APBPrescaler::Div16 => 0x07,
        }
    }
}

impl Into<u8> for AHBPrescaler {
    fn into(self) -> u8 {
        match self {
            AHBPrescaler::NotDivided => 1,
            AHBPrescaler::Div2 => 0x08,
            AHBPrescaler::Div3 => 0x01,
            AHBPrescaler::Div4 => 0x09,
            AHBPrescaler::Div5 => 0x02,
            AHBPrescaler::Div6 => 0x05,
            AHBPrescaler::Div8 => 0x0a,
            AHBPrescaler::Div10 => 0x06,
            AHBPrescaler::Div16 => 0x0b,
            AHBPrescaler::Div32 => 0x07,
            AHBPrescaler::Div64 => 0x0c,
            AHBPrescaler::Div128 => 0x0d,
            AHBPrescaler::Div256 => 0x0e,
            AHBPrescaler::Div512 => 0x0f,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum RtcClkSrc {
    None = 0b00,
    Lse = 0b01,
    Lsi = 0b10,
    HseDiv32 = 0b11,
}

/// CPU1, CPU2 HPRE (prescaler).
/// RM0434 page 230.
#[derive(Debug, Copy, Clone)]
pub enum HDivider {
    NotDivided = 0,
    Div2 = 0b1000,
    Div3 = 0b0001,
    Div4 = 0b1001,
    Div5 = 0b0010,
    Div6 = 0b0101,
    Div10 = 0b0110,
    Div8 = 0b1010,
    Div16 = 0b1011,
    Div32 = 0b0111,
    Div64 = 0b1100,
    Div128 = 0b1101,
    Div256 = 0b1110,
    Div512 = 0b1111,
}

impl HDivider {
    /// Returns division value
    pub fn divisor(&self) -> u32 {
        match self {
            HDivider::NotDivided => 1,
            HDivider::Div2 => 2,
            HDivider::Div3 => 3,
            HDivider::Div4 => 4,
            HDivider::Div5 => 5,
            HDivider::Div6 => 6,
            HDivider::Div10 => 10,
            HDivider::Div8 => 8,
            HDivider::Div16 => 16,
            HDivider::Div32 => 32,
            HDivider::Div64 => 64,
            HDivider::Div128 => 128,
            HDivider::Div256 => 256,
            HDivider::Div512 => 512,
        }
    }
}

/// PLL configuration.
#[derive(Debug, Clone)]
pub struct PllConfig {
    pub m: u8,
    pub n: u8,
    pub r: u8,
    pub q: Option<u8>,
    pub p: Option<u8>,
}

impl Default for PllConfig {
    fn default() -> Self {
        PllConfig {
            m: 1,
            n: 8,
            r: 2,
            q: None,
            p: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StopWakeupClock {
    MSI = 0,
    HSI16 = 1,
}

/// Clocks configutation
pub struct Config {
    mux: ClockSrc,
    cpu1_hdiv: HDivider,
    cpu2_hdiv: HDivider,
    ahb_pre: AHBPrescaler,
    apb1_pre: APBPrescaler,
    apb2_pre: APBPrescaler,
    pll_config: PllConfig,
    stop_wakeup_clk: StopWakeupClock,
    with_lse: bool,
    rtc_src: RtcClkSrc,
}

impl Default for Config {
    #[inline]
    fn default() -> Config {
        Config {
            mux: ClockSrc::HSI16,
            cpu1_hdiv: HDivider::NotDivided,
            cpu2_hdiv: HDivider::NotDivided,
            ahb_pre: AHBPrescaler::NotDivided,
            apb1_pre: APBPrescaler::NotDivided,
            apb2_pre: APBPrescaler::NotDivided,
            pll_config: PllConfig::default(),
            stop_wakeup_clk: StopWakeupClock::MSI,
            with_lse: false,
            rtc_src: RtcClkSrc::None,
        }
    }
}

impl Config {
    #[inline]
    pub fn clock_src(mut self, mux: ClockSrc) -> Self {
        self.mux = mux;
        self
    }

    #[inline]
    pub fn cpu1_hdiv(mut self, div: HDivider) -> Self {
        self.cpu1_hdiv = div;
        self
    }

    #[inline]
    pub fn cpu2_hdiv(mut self, div: HDivider) -> Self {
        self.cpu2_hdiv = div;
        self
    }

    #[inline]
    pub fn ahb_pre(mut self, pre: AHBPrescaler) -> Self {
        self.ahb_pre = pre;
        self
    }

    #[inline]
    pub fn apb1_pre(mut self, pre: APBPrescaler) -> Self {
        self.apb1_pre = pre;
        self
    }

    #[inline]
    pub fn apb2_pre(mut self, pre: APBPrescaler) -> Self {
        self.apb2_pre = pre;
        self
    }

    #[inline]
    pub fn pll_config(mut self, pll_config: PllConfig) -> Self {
        self.pll_config = pll_config;
        self
    }

    #[inline]
    pub fn stop_wakeup_clk(mut self, stop_wakeup_clk: StopWakeupClock) -> Self {
        self.stop_wakeup_clk = stop_wakeup_clk;
        self
    }

    #[inline]
    pub fn with_lse(mut self, with_lse: bool) -> Self {
        self.with_lse = with_lse;
        self
    }

    #[inline]
    pub fn rtc_src(mut self, src: RtcClkSrc) -> Self {
        self.rtc_src = src;
        self
    }
}

/// RCC peripheral
pub struct Rcc<'d> {
    _rb: peripherals::RCC,
    phantom: PhantomData<&'d mut peripherals::RCC>,
}

impl<'d> Rcc<'d> {
    pub fn new(rcc: impl Unborrow<Target = peripherals::RCC> + 'd) -> Self {
        unborrow!(rcc);
        Self {
            _rb: rcc,
            phantom: PhantomData,
        }
    }

    // Safety: RCC init must have been called
    pub fn clocks(&self) -> &'static Clocks {
        unsafe { get_freqs() }
    }
}

/// Extension trait that freezes the `RCC` peripheral with provided clocks configuration
pub trait RccExt {
    fn freeze(self, config: Config) -> Clocks;
}

impl RccExt for RCC {
    #[inline]
    fn freeze(self, cfgr: Config) -> Clocks {
        let rcc = pac::RCC;

        let mut pll_clk = None;
        let mut pllp_clk = None;
        let mut pllq_clk = None;

        // Enable backup domain access to access LSE/RTC registers
        unsafe {
            // ST: Write twice the value to flush the APB-AHB bridge to ensure the bit is written
            PWR.cr1().modify(|r| r.set_dbp(true));
            PWR.cr1().modify(|r| r.set_dbp(true));
        }

        let lse_freq = if cfgr.with_lse {
            unsafe {
                rcc.bdcr().modify(|r| r.set_lseon(true));

                while !rcc.bdcr().read().lserdy() {}
            }

            Some(32_768.hz())
        } else {
            None
        };

        let bit = match cfgr.stop_wakeup_clk {
            StopWakeupClock::MSI => false,
            StopWakeupClock::HSI16 => true,
        };

        // set wakeup clock
        unsafe { rcc.cfgr().modify(|r| r.set_stopwuck(bit)) }

        let (sys_clk, sw_bits) = match cfgr.mux {
            ClockSrc::HSI16 => {
                // Enable HSI16
                unsafe {
                    rcc.cr().write(|w| w.set_hsion(true));
                    while !rcc.cr().read().hsirdy() {}
                }

                (HSI_FREQ, 0x01)
            }
            ClockSrc::HSE(div) => {
                let (divided, f_input) = match div {
                    HseDivider::NotDivided => (false, HSE_FREQ),
                    HseDivider::Div2 => (true, HSE_FREQ / 2),
                };

                // Configure HSE divider and enable it
                unsafe {
                    rcc.cr().modify(|r| {
                        r.set_hsepre(divided);
                        r.set_hseon(true);
                    });

                    // Wait for HSE startup
                    while !rcc.cr().read().hserdy() {}
                }

                (f_input, 0x02)
            }
            ClockSrc::Pll(src) => {
                // Configure PLL
                //self.configure_and_wait_for_pll(&cfgr.pll_config, &src);
                let config = &cfgr.pll_config;

                // determine input frequency for PLL
                // Select PLL and PLLSAI1 clock source [RM0434, p. 233]
                let (f_input, src_bits) = match src {
                    PllSrc::Msi(_range) => {
                        todo!();

                        /*
                        let f_input = 0;
                        (f_input, 0b01)
                        */
                    }
                    PllSrc::Hsi => (HSI_FREQ, 0b10),
                    PllSrc::Hse(div) => {
                        let (divided, f_input) = match div {
                            HseDivider::NotDivided => (false, HSE_FREQ),
                            HseDivider::Div2 => (true, HSE_FREQ / 2),
                        };

                        // Configure HSE divider and enable it
                        unsafe {
                            rcc.cr().modify(|r| {
                                r.set_hsepre(divided);
                                r.set_hseon(true);
                                r.set_csson(true);
                            });

                            // Wait for HSE startup
                            while !rcc.cr().read().hserdy() {}
                        }

                        (f_input, 0b11)
                    }
                };

                let pllp = config.p.map(|p| {
                    assert!(p > 1);
                    assert!(p <= 32);
                    (p - 1) & 0b11111
                });

                let pllq = config.q.map(|q| {
                    assert!(q > 1);
                    assert!(q <= 8);
                    (q - 1) & 0b111
                });

                // Set R value
                assert!(config.r > 1);
                assert!(config.r <= 8);
                let pllr = (config.r - 1) & 0b111;

                // Set N value
                assert!(config.n > 7);
                assert!(config.n <= 86);
                let plln = config.n & 0b1111111;

                // Set M value
                assert!(config.m > 0);
                assert!(config.m <= 8);
                let pllm = (config.m - 1) & 0b111;

                let vco = f_input / config.m as u32 * config.n as u32;
                let f_pllr = vco / config.r as u32;

                assert!(f_pllr <= 64_000_000);

                pll_clk = Some(f_pllr.hz());

                if let Some(pllp) = pllp {
                    let f_pllp = vco / (pllp + 1) as u32;
                    assert!(f_pllp <= 64_000_000);

                    pllp_clk = Some(f_pllp.hz());
                }

                if let Some(pllq) = pllq {
                    let f_pllq = vco / (pllq + 1) as u32;
                    assert!(f_pllq <= 64_000_000);

                    pllq_clk = Some(f_pllq.hz());
                }

                unsafe {
                    // Set PLL coefficients
                    rcc.pllcfgr().modify(|r| {
                        r.set_pllsrc(src_bits);
                        r.set_pllm(pllm);
                        r.set_plln(plln);
                        r.set_pllr(pllr);
                        r.set_pllren(true);
                        r.set_pllp(pllp.unwrap_or(1));
                        r.set_pllpen(pllp.is_some());
                        r.set_pllq(pllq.unwrap_or(1));
                        r.set_pllqen(pllq.is_some());
                    });

                    // Enable PLL and wait for setup
                    rcc.cr().modify(|r| r.set_pllon(true));
                    while !rcc.cr().read().pllrdy() {}
                }

                (f_pllr, 0b11)
                //(HSE_FREQ, 0x02)
            }
            ClockSrc::Msi => todo!(),
        };

        // Configure FLASH wait states
        unsafe {
            FLASH.acr().write(|w| {
                w.set_latency(if sys_clk <= 18_000_000 {
                    0
                } else if sys_clk <= 36_000_000 {
                    1
                } else if sys_clk <= 54_000_000 {
                    2
                } else {
                    3
                })
            });
        }

        defmt::info!("Enabled PLL (sysclk={})!", sys_clk);

        unsafe {
            rcc.cfgr().modify(|w| {
                w.set_sw(sw_bits.into());
            });

            while rcc.cfgr().read().sw() != sw_bits {}
        }

        unsafe {
            rcc.cfgr().modify(|w| {
                w.set_hpre(cfgr.ahb_pre.into());
                w.set_ppre1(cfgr.apb1_pre.into());
                w.set_ppre2(cfgr.apb2_pre.into());
            });
        }

        let ahb_freq: u32 = match cfgr.ahb_pre {
            AHBPrescaler::NotDivided => sys_clk,
            pre => {
                let pre: u8 = pre.into();
                let pre = 1 << (pre as u32 - 7);
                sys_clk / pre
            }
        };

        let (apb1_freq, apb1_tim_freq) = match cfgr.apb1_pre {
            APBPrescaler::NotDivided => (ahb_freq, ahb_freq),
            pre => {
                let pre: u8 = pre.into();
                let pre: u8 = 1 << (pre - 3);
                let freq = ahb_freq / pre as u32;
                (freq, freq * 2)
            }
        };

        let (apb2_freq, apb2_tim_freq) = match cfgr.apb2_pre {
            APBPrescaler::NotDivided => (ahb_freq, ahb_freq),
            pre => {
                let pre: u8 = pre.into();
                let pre: u8 = 1 << (pre - 3);
                let freq = ahb_freq / (1 << (pre as u8 - 3));
                (freq, freq * 2)
            }
        };

        let cpu1_freq = match cfgr.cpu1_hdiv {
            HDivider::NotDivided => (sys_clk),
            div => {
                let div = div.divisor();
                sys_clk / div
            }
        };

        unsafe {
            rcc.cfgr().modify(|r| r.set_hpre(cfgr.cpu1_hdiv as u8));
            rcc.extcfgr().modify(|r| r.set_c2hpre(cfgr.cpu2_hdiv as u8));

            // Wait for prescaler values to apply
            while !rcc.cfgr().read().hpref() {}
            while !rcc.extcfgr().read().shdhpref() {}
        }

        let cpu2_freq = match cfgr.cpu2_hdiv {
            HDivider::NotDivided => (sys_clk),
            div => {
                let div = div.divisor();
                sys_clk / div
            }
        };

        Clocks {
            sys: sys_clk.hz(),
            ahb1: ahb_freq.hz(),
            ahb2: ahb_freq.hz(),
            ahb3: ahb_freq.hz(),
            apb1: apb1_freq.hz(),
            apb2: apb2_freq.hz(),
            apb1_tim: apb1_tim_freq.hz(),
            apb2_tim: apb2_tim_freq.hz(),
            pll_clk,
            pllp: pllp_clk,
            pllq: pllq_clk,
            cpu_1: cpu1_freq.hz(),
            cpu_2: cpu2_freq.hz(),
            lse: lse_freq,
        }
    }
}

pub unsafe fn init(config: Config) {
    let r = <peripherals::RCC as embassy::util::Steal>::steal();
    let clocks = r.freeze(config);
    set_freqs(clocks);
}
