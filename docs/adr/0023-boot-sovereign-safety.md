# ADR-0023: Boot-Sovereign safety model & format/bootloader tradeoffs

## Status
Accepted (Phase 14)

## Context
Chimera needs a Rufus-grade USB flashing / recovery engine (`chimera-boot` in `crates/usb-flasher`). Raw block writes can destroy host disks. Development runs on a real primary machine.

## Decision — Safety gates (non-negotiable)
Physical writes require **all** of:
1. Explicit `--yes-i-understand-this-destroys-data` **or** typed disk serial confirmation
2. Removable-media check that **hard-refuses** fixed disks and system/boot volumes
3. `--no-dry-run` (dry-run is **ON** by default)

`FileImageTarget` is the only path exercised by automated tests. Enumeration (`usb list`) is read-only and safe.

**Real-hardware flashing is UNTESTED** in CI and development verification.

## Decision — Formats
| Surface | Approach |
|---|---|
| MBR | Spec-correct writer/parser (tested on file images) |
| GPT | Header + entries + protective MBR + CRC32 + backup (tested on file images) |
| FAT32 | In-crate BPB/FSInfo/FAT/root writer (tested on file images) |
| NTFS | **Not** implemented in-process — delegate to OS (`format` / `mkfs.ntfs`). Selecting NTFS returns a clear error. |

## Decision — Bootloaders
Zero precompiled GRUB/Syslinux/EFI blobs in-tree. User supplies:
- EFI stub path → materialized as `EFI/BOOT/BOOTX64.EFI` in an ESP tree
- Optional 440-byte legacy MBR bootstrap (default = zeros / no-op)

## Consequences
- Lab flashing uses `--image` file targets
- Physical path compiles on Windows/Linux but must never be used without gates
- SMART thermal is reported as unavailable rather than fabricated
