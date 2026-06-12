pub mod quantize {
    #[derive(Debug, Clone, Copy)]
    pub enum QuantMode {
        Int8,
        Int4,
        NF4,
    }
    pub struct Quantizer;
    impl Quantizer {
        pub fn new() -> Self { Self }
    }
    pub struct QuantizedTensor;
}
