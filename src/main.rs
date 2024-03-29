#![no_std]
#![no_main]


use cortex_m_rt::entry;
use defmt::*;
use defmt_rtt as _; // global logger
use panic_probe as _; // panic handler

use embassy_executor::{Executor, InterruptExecutor};
use embassy_time::{Duration, Ticker, Timer};
use embassy_stm32::{
    exti::ExtiInput, 
    gpio::{Level, Output, Speed, Pull},
    interrupt,
    interrupt::{InterruptExt, Priority}, 
    peripherals, 
    time::Hertz, 
    wdg::IndependentWatchdog, Peripherals

};

use assign_resources::assign_resources;
use static_cell::StaticCell;

assign_resources! {
    led:    LedRes {green: PA5}
    exti:   ExtRes {dev: EXTI2,pin: PB2, ack: PB13}
    stim:   StiRes {stim: PB1, sent: PB14}
    wdog:   WdgRes {cat: IWDG}
}

#[embassy_executor::task]
async fn exti_task(r: ExtRes){
    let mut ack = Output::new(r.ack, Level::Low, Speed::High);
    let mut pin = ExtiInput::new(r.pin, r.dev, Pull::Down);
    loop {
        info!("loop reached.");
        pin.wait_for_rising_edge().await;
        ack.set_high();
        info!("Got Exti!");
        ack.set_low();
    }
}

#[embassy_executor::task]
async fn stimulus_task(r: StiRes){
    let mut stimulus = Output::new(r.stim, Level::Low, Speed::High);
    let mut sent = Output::new(r.sent, Level::Low, Speed::High);
    let mut ticker = Ticker::every(Duration::from_millis(20));
    loop {
        ticker.next().await;
        stimulus.toggle();
        sent.toggle();
        info!("Sent Exti");
        
    }
}

#[embassy_executor::task]
async fn led_task(r: LedRes) {
    let mut led = Output::new(r.green, Level::Low, Speed::Low);
    let mut ticker = Ticker::every(Duration::from_millis(500));
    loop {
        ticker.next().await; 
        led.toggle();
        info!("Led Toggle");
    }
}

#[embassy_executor::task]
async fn pet_cat(r: WdgRes){
    let mut cat = IndependentWatchdog::new(r.cat, 20_000_000);
    cat.unleash();
    loop {
        Timer::after_secs(10).await;
        cat.pet();
    }
}

static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();

#[allow(non_snake_case)]
#[interrupt]
unsafe fn UART4() {
    EXECUTOR_HIGH.on_interrupt()
}

fn init_clocks() -> Peripherals {
    info!("init clocks started.");
    use embassy_stm32::rcc::*;
    use embassy_stm32::Config;
    let mut config = Config::default();
    config.rcc.hse = Some(Hse{            
        freq: Hertz(8_000_000),
        mode: HseMode::Bypass
    });
    config.rcc.pll_src = PllSource::HSE;
    config.rcc.pll = Some(Pll {
        prediv: PllPreDiv::DIV4,
        mul: PllMul::MUL168,
        divp: Some(PllPDiv::DIV2), // 8mhz / 4 * 168 / 2 = 168Mhz.
        divq: None, 
        divr: None,
    });
    config.rcc.ahb_pre = AHBPrescaler::DIV1;
    config.rcc.apb1_pre = APBPrescaler::DIV4;
    config.rcc.apb2_pre = APBPrescaler::DIV2;
    config.rcc.sys = Sysclk::PLL1_P;
    embassy_stm32::init(config)
}

#[entry]
fn main() -> ! {
    info!("main started.");
    let p = init_clocks();
    let r = split_resources!(p);
    info!("resources assigned.");

    // High-priority executor: UART4, priority level 6
    interrupt::UART4.set_priority(Priority::P6);
    let spawner = EXECUTOR_HIGH.start(interrupt::UART4);
    info!("interrupt spawner created.");
    unwrap!(spawner.spawn(exti_task(r.exti)));
    info!("interrupt task spawned.");

    // Low priority executor: runs in thread mode, using WFE/SEV
    let executor = EXECUTOR_LOW.init(Executor::new());
    info!("main executor created.");
    executor.run(|spawner| {
        unwrap!(spawner.spawn(stimulus_task(r.stim)));
        unwrap!(spawner.spawn(led_task(r.led)));
        unwrap!(spawner.spawn(pet_cat(r.wdog)));
    //    unwrap!(spawner.spawn(exti_task(r.exti)));
    });
}