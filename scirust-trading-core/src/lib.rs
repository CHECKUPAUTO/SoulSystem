//! SciRust Trading Core
pub mod types {
    #[derive(Clone)]
    pub struct Bar;
    #[derive(Clone)]
    pub struct Order;
    #[derive(Clone)]
    pub struct Trade;
}
pub mod market {
    #[derive(Clone)]
    pub struct MarketState;
}
pub mod codified {
    #[derive(Clone)]
    pub struct CodifiedEvent;
}
pub mod bus;
