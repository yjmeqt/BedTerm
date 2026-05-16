# BedTerm MVP — Post-MVP Backlog

Items deferred out of the MVP. Not part of the PRD schema; tracked here for reference.

- **multi_host** — Multi-host list with saved credentials per host. MVP stores only one host record; the data model is designed to extend to an array.
- **auto_reconnect** — Auto-reconnect / mosh-style session recovery on transient network loss.
- **port_forward** — Port forwarding, SFTP, scp.
- **theme_settings** — Theme and font-size settings UI.
- **custom_keybar** — User-customizable key bar (add/remove keys, reorder).
- **ipad_layout** — iPad-optimized layout.
- **ctrl_not_ctrl_revisit** — Revisit R4.ctrl_not_ctrl once real usage clarifies whether any program needs a literal Ctrl interaction.
- **ecdsa_keys** — Support ECDSA (p256/p384/p521) OpenSSH private keys. Citadel 0.12.1 only parses ed25519 and RSA OpenSSH key files; users importing ECDSA keys currently get `.privateKeyParse`. Either wait for Citadel upstream or parse them ourselves and pass pre-built CryptoKit `P*.Signing.PrivateKey` into the `.p256` / `.p384` / `.p521` Citadel auth factories.
- **key_passphrase_error** — Distinguish wrong-passphrase from corrupt-key for encrypted OpenSSH private keys. Citadel's `InvalidOpenSSHKey` does not say why decryption failed; current code heuristically maps "no passphrase + bcrypt/cipher/decrypt/kdf in error text" to `.privateKeyPassphraseRequired` and everything else to `.privateKeyParse`, so a wrong-passphrase user sees a generic parse error.
