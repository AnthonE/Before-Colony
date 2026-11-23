//! A signature never prints: its `Debug` says what it is and how long, never its bytes, and so does
//! any message that carries one. What a log line or a panic writes can't leak it (`CLAUDE.md`, "never
//! log signatures or resume tokens"; `SECURITY.md`).

use bc_proto::auth::{Address, SOLANA_KEY_BYTES, Signature, SolanaSignature};
use bc_proto::control::ControlMsg;

#[test]
fn a_signature_prints_its_length_never_its_bytes() {
    assert_eq!(format!("{:?}", Signature([0xAB; 65])), "Signature(65 bytes)");
    assert_eq!(format!("{:?}", SolanaSignature([0xAB; 64])), "SolanaSignature(64 bytes)");
}

#[test]
fn a_sign_in_prints_its_address_but_not_its_signature() {
    // 0xAB is 171 in the derived `Debug`'s decimal: the signature's bytes, if they showed.
    let evm =
        format!("{:?}", ControlMsg::Auth { address: Address([0x11; 20]), signature: Signature([0xAB; 65]) });
    assert!(evm.contains("Signature(65 bytes)") && !evm.contains("171"), "{evm}");
    let sol = format!(
        "{:?}",
        ControlMsg::AuthSolana { key: [0x11; SOLANA_KEY_BYTES], signature: SolanaSignature([0xAB; 64]) }
    );
    assert!(sol.contains("SolanaSignature(64 bytes)") && !sol.contains("171"), "{sol}");
}
