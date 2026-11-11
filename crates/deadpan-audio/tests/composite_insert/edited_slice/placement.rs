//! Atomic edited-slice placement, with destination allocations derived directly
//! from absolute frame boundaries and independent retained source support.

// Source natural-rate extent is 1601.6 mix points per project frame. The
// admitted provider endpoint is ceil(100 + frames*1471.47) original samples.
// Keep that complete endpoint for reconstruction, including the final point.
// A transferred RootRoundEven operand additionally needs its retained discrete
// root support, checked separately in the complete-owner terminal fixture below.
