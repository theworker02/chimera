# usb-flasher (`chimera-boot`)

Chimera Boot-Sovereign — Rufus-grade **safety-gated** USB flash / recovery engine.

## ⚠️ Data-loss warning

This crate can **destroy all data** on a target disk. Defaults:

- `--dry-run` **ON**
- Physical writes require `--yes-i-understand-this-destroys-data` **and** `--no-dry-run`
- Non-removable / system disks are **hard-refused**

**All automated tests use file-backed virtual disk images only.** Real-hardware flashing is **UNTESTED**.

## Pure source

No GRUB/Syslinux/EFI binaries are bundled. Supply bootloader files as paths.

```bash
cargo test -p chimera-boot
cargo run -p cli-tool -- usb list
```

See [ADR-0023](../../docs/adr/0023-boot-sovereign-safety.md).
