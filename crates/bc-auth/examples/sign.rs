//! Signs stdin the way a browser wallet's `personal_sign` does, with a throwaway test key (the
//! secret is the number given, 1 by default). The browser tests' stub wallet signs through it
//! (`e2e/tests/login.spec.ts`), so the page's sign-in runs against a real signature.
//!
//! ```sh
//! printf 'hello' | cargo run -p bc-auth --example sign -- 1
//! cargo run -p bc-auth --example sign -- 1 address
//! ```

use std::io::Read;

use bc_auth::LocalWallet;

fn main() {
    let mut args = std::env::args().skip(1);
    let k: u8 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);
    let mut secret = [0u8; 32];
    secret[31] = k;
    let wallet = LocalWallet::from_secret(&secret).expect("a valid test key");
    if args.next().as_deref() == Some("address") {
        print!("{}", String::from_utf8_lossy(&bc_auth::lower_hex(&wallet.address())));
        return;
    }
    let mut message = Vec::new();
    std::io::stdin().read_to_end(&mut message).expect("the message on stdin");
    let hex: String = wallet.sign(&message).0.iter().map(|b| format!("{b:02x}")).collect();
    print!("0x{hex}");
}
