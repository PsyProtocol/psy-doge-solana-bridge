use bytemuck::{Pod, Zeroable};
use psy_doge_solana_core::program_state::PsyBridgeProgramState;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct BridgeState {
    pub core_state: PsyBridgeProgramState,
    pub doge_mint: [u8; 32],
}

impl BridgeState {
    pub const SIZE: usize = std::mem::size_of::<BridgeState>();
}
