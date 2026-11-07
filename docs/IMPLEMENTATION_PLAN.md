# Arcaforty implementation plan

## Product
A Windows-first native Rust desktop vault built with GPUI. Store API keys, passwords, environment blocks, recovery codes, seed phrases, private keys, and secure notes. Keep all data local. Do not add cloud sync or telemetry.

## Design
Reference palette: ivory #F7F5EF, paper #FFFEFA, ink #191912, muted #8E8A7D, line #DED8CB, green #4E805F. Georgia headings, Segoe UI controls, Consolas secret values. Use generous space, quiet borders, and small corner radii. Center setup and unlock. Left-align list and editor content. Use a settings sidebar. No decorative captions, gradients, or dashboard cards.

Vault layout:
```text
Arcaforty                            Settings
[ Search secrets                           ]
[All] [API keys] [Passwords] [More kinds ...]
Platform / notes                Kind   Copy
Platform / notes                Kind   Copy
-------------------------------------------
Lock vault                       New secret
```

Design review: The warm palette and serif type follow the supplied references. Use a searchable list instead of a dashboard. Keep the memorable serif treatment on screen titles. Use sentence case labels instead of the reference presentation captions.

## Implementation sequence
1. Establish a Rust package and separate vault core from GPUI presentation.
2. Encrypt the complete vault payload with AES-256-GCM. Derive keys with Argon2id, random salt, and fresh nonces. Validate file headers and bound input sizes. Write atomically. Zeroize sensitive owned buffers where practical.
3. Implement setup, unlock, search and kind filters, add/edit/delete, copy, timed reveal, and manual lock.
4. Implement configurable idle lock, clipboard timeout, reveal duration, appearance, password change, encrypted backup export/restore, and reviewed .env/CSV import.
5. Verify wrong-password rejection, tamper detection, persistence, password rotation, parsing, and bounds. Compile and run the native app when host tools permit. Document exact build steps and remaining limitations.

## Safety and scope
Never seed a real vault with sample credentials. Never log passwords or secret values. Do not claim biometric unlock, sleep integration, or secure memory guarantees without working implementation and verification. The first release needs independent security review before users rely on it for critical secrets.

## Delegation
Sonnet 5.5: implement the Rust vault core and GPUI app, then compile and test.
GPT-6-Luna: inspect Windows prerequisites and GPUI documentation, and report build risks and concrete guidance without changing implementation files.
