use crate::codified::CodifiedEvent;
use crate::market::MarketState;
use crate::types::{Bar, Order, Trade};
use tokio::sync::broadcast;

pub enum TradingEvent {
    Market(MarketState),
    News(CodifiedEvent),
    Trade(Trade),
    OrderUpdate(Order),
    Bar(Bar),
}

#[derive(Clone)]
pub struct EventBus {
    pub market: broadcast::Sender<MarketState>,
    pub news: broadcast::Sender<CodifiedEvent>,
    pub trades: broadcast::Sender<Trade>,
    pub orders: broadcast::Sender<Order>,
    pub bars: broadcast::Sender<Bar>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::with_capacities(1024, 256, 4096, 1024, 1024)
    }

    pub fn with_capacities(
        market_cap: usize,
        news_cap: usize,
        trades_cap: usize,
        orders_cap: usize,
        bars_cap: usize,
    ) -> Self {
        let (market, _) = broadcast::channel(market_cap);
        let (news, _) = broadcast::channel(news_cap);
        let (trades, _) = broadcast::channel(trades_cap);
        let (orders, _) = broadcast::channel(orders_cap);
        let (bars, _) = broadcast::channel(bars_cap);
        Self {
            market,
            news,
            trades,
            orders,
            bars,
        }
    }

    pub fn subscribe(&self) -> EventBusHandle {
        EventBusHandle {
            market_rx: self.market.subscribe(),
            news_rx: self.news.subscribe(),
            trades_rx: self.trades.subscribe(),
            orders_rx: self.orders.subscribe(),
            bars_rx: self.bars.subscribe(),
        }
    }
}

pub struct EventBusHandle {
    pub market_rx: broadcast::Receiver<MarketState>,
    pub news_rx: broadcast::Receiver<CodifiedEvent>,
    pub trades_rx: broadcast::Receiver<Trade>,
    pub orders_rx: broadcast::Receiver<Order>,
    pub bars_rx: broadcast::Receiver<Bar>,
}
