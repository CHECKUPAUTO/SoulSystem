//! Demo: create market state, bars, and compute reactions.
//!
//! cargo run --example market_demo -p scirust-trading-core

use chrono::Utc;
use scirust_trading_core::{
    Category, CodifiedEvent, EventTiming, Exchange, MarketReaction, MarketState, Polarity, Symbol,
};

fn main() {
    println!("scirust trading core — market demo");
    println!("══════════════════════════════════════════════════════════════");

    // 1. Create a market state
    let state = MarketState {
        exchange: Exchange::Binance,
        symbol: Symbol::new("BTC", "USDT"),
        timestamp: Utc::now(),
        mid: 68_500.0,
        microprice: 68_502.3,
        spread_bps: 0.8,
        imbalance_5: 0.15,
        imbalance_20: 0.05,
        realized_vol_pct: 32.5,
        volume_1min: 12.5,
        flow_imbalance_1min: 0.22,
        trade_count_1min: 85,
    };

    println!("\n[1] MarketState:");
    println!(
        "    {} {} @ {:.2}",
        state.exchange_str(),
        state.symbol.pair(),
        state.mid
    );
    println!(
        "    spread: {:.1} bps, vol: {:.1}%",
        state.spread_bps, state.realized_vol_pct
    );
    println!(
        "    imbalance(5): {:.2}, imbalance(20): {:.2}",
        state.imbalance_5, state.imbalance_20
    );

    // 2. Create a CodifiedEvent via builder
    let event = CodifiedEvent::builder("reuters", "Fed raises rates by 25bp")
        .title("FOMC Rate Decision")
        .category(Category::Macro)
        .timing(EventTiming::Observed(Utc::now()))
        .reliability(0.95)
        .tag("fomc")
        .tag("rate_decision")
        .build();

    println!("\n[2] CodifiedEvent:");
    println!("    title: {}", event.title);
    println!("    category: {:?}", event.category);
    println!(
        "    source: {}, reliability: {:.2}",
        event.source,
        event.source_reliability.value()
    );

    // 3. Simulate a market reaction
    let reaction = MarketReaction {
        direction: 1,
        magnitude: 250.0,
        confidence: 0.85,
        timestamp_ms: Utc::now().timestamp_millis(),
        n_samples: 15,
        delta_5min_bps: 12.5,
        delta_15min_bps: 25.3,
        delta_60min_bps: 45.8,
        delta_60min_std_bps: 15.2,
        volume_spike_ratio: 3.2,
    };

    println!("\n[3] MarketReaction:");
    println!(
        "    direction: {} ({}), magnitude: {:.0} bps",
        reaction.direction,
        if reaction.direction > 0 { "up" } else { "down" },
        reaction.magnitude
    );
    println!(
        "    Δ+5m: {:.1} bps, Δ+15m: {:.1} bps, Δ+60m: {:.1} ± {:.1} bps",
        reaction.delta_5min_bps,
        reaction.delta_15min_bps,
        reaction.delta_60min_bps,
        reaction.delta_60min_std_bps
    );
    println!(
        "    volume spike: {:.1}x, significant: {}",
        reaction.volume_spike_ratio,
        reaction.is_significant()
    );

    // 4. Weighted score
    let mut enriched = event.clone();
    enriched.polarity = Some(Polarity::new(-0.6));
    enriched.magnitude = Some(0.8);
    enriched.semantic_confidence = Some(0.9);
    enriched.historical_response = Some(reaction);

    let score = enriched.weighted_score(Utc::now());
    println!("\n[4] Weighted Score:");
    println!("    polarity: {:.2}", enriched.polarity.unwrap().value());
    println!("    magnitude: {:.2}", enriched.magnitude.unwrap());
    println!(
        "    confidence: {:.2}",
        enriched.semantic_confidence.unwrap()
    );
    println!("    score: {:+.4}", score);

    println!("\n══════════════════════════════════════════════════════════════");
    println!("Market demo complete.");
}
