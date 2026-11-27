# Checksum Engines and JSON Plugins

`kivi-checksum` verifies and repairs ROM integrity blocks before saving or flashing an edited ECU binary. It provides 32 registered OEM engines, XDF checksum block evaluation, and declarative JSON custom checksum plugins so you can support additional ROM layouts without recompiling Rust.

## 1. Built-In OEM Checksum Engines

Every registered engine below supports both verification (`verify_oem_by_id`) and repair (`repair_oem_by_id`), alongside automatic layout detection (`auto_detect_oem_checksums`).

### Motorcycle and Powersports OEM (8 Engines)

| Algorithm ID | Engine Name | Scope and Algorithm |
|---|---|---|
| `honda_keihin_moto` | Honda (Keihin) | X-ADV 750 (`512 KiB` MKH) and ADV350 (`768 KiB` KN6) 8-bit sum = 0 mod 256 |
| `yamaha_mt` | Yamaha MT series | MT-25 / MT-07 / MT-09 16-bit BE word sum = `0x5AA5` (`1 MiB`) or `0xE81F` (`896 KiB`) |
| `bosch_mse6` | Bosch MSE6.0 | 16-bit BE word sum across `[start][end][cs][~cs]` descriptors |
| `bosch_mse8` | Bosch MSE8.0 | `256 KiB` block CRC-32 XOR `0xA18AFC77` with `1234567812341008` footer |
| `bosch_mg1` | Bosch MG1 (`MG1CA094`) | CRC-32 (seed `0x533F71C8`) over `DEADBEEF` blocks (supports standard update and GF(2) keep-CRC modes) |
| `dellorto_ecm` | Dell'Orto ECM | Bajaj/Dell'Orto `0xC0000` image `~byte_sum` of `0x80000..0x921FD` stored at `0x921FE` |
| `italjet_suzuki` | Italjet / Suzuki-type ECU | `0x80000` image 16-bit LE byte sum `0x20000..end` with complement near `AA55CC33` |
| `qh250` | QH250 (`E2400`) | `0x140000` image 16-bit BE word sum of `0..0x7FEF = 0` stored at `0x7FEE` |

### Siemens BMW and Mini (5 Engines)

| Algorithm ID | Engine Name | Scope and Algorithm |
|---|---|---|
| `bmw_ms41` | BMW Siemens MS41 | `256 KiB` full / `24 KiB` partial CRC-16/ARC boot and calibration tables |
| `bmw_ms43` | BMW Siemens MS43 | `512 KiB` boot/program/calibration CRC-16/ARC descriptors |
| `bmw_gs20` | BMW Siemens GS20 TCU | `64 KiB` calibration (BE16 at `0x0D`) / `256 KiB` program CRC-16/ARC |
| `bmw_smg2` | BMW Siemens SMG II TCU | `32 KiB` data CRC-16/ARC init `0x7878` at `8320` |
| `mini_ems2k` | Mini Siemens EMS2000 | `512 KiB` 3x CRC-16/ARC (calibration at `0x77FE0` repairable) |

### GM Delco PCM (1 Multi-Family Engine)

| Algorithm ID | Engine Name | Scope and Algorithm |
|---|---|---|
| `gm_pcm` | GM Delco PCM | Auto-detects P01, P59, P08, P10, P12, E38, E39, E92, and E54 segment word sums and CRC-16 CVNs (aliases `gm_p01`, `gm_p59`, `gm_e38`, `gm_e39`, `gm_e92` also accepted) |

### Bosch, VAG, and Sagem (10 Engines)

| Algorithm ID | Engine Name | Scope and Algorithm |
|---|---|---|
| `bosch_edc15` | Bosch VAG EDC15P/V/VM | TDI v4.1 / v4.1v2 / v4.1-2002 rotating XOR/sub zone checksums |
| `bosch_edc16` | Bosch EDC16U (`FADECAFE`) | Descriptor-based BE32 sum = `0xD01FE500` across `0x100`-aligned regions |
| `vag_haldex_cal` | VAG Haldex 4Motion CAL (block 2) | 16-bit LE one's-complement word sum stored at `0x18` |
| `vag_haldex_asw` | VAG Haldex 4Motion ASW (block 3) | 16-bit LE one's-complement word sum stored at `0x208` |
| `vag_me7xx_crc` | VAG Bosch ME7.XX CRC-32 | C166 signature-driven 3-zone ME7.x CRC-32 verifier and repair |
| `bosch_me3x` | Bosch M3.x / M5.x | `128/256 KiB` page-marker byte sum verifier and repair |
| `bosch_m797` | Hyundai Bosch M7.9.7 | `512 KiB` 5-zone LE16 sum at `0x7FFE0` |
| `bosch_china_m797` | China Bosch M7.9.7 | `1 MiB` 2-zone LE16 sum at `0xFFFE8` |
| `bosch_me745` | Citroen Bosch ME7.4.5 | `832 KiB` page blocks + 3x CRC-32 |
| `sagem_samand` | Sagem Iran Khodro Samand | `832 KiB` dual `sum8` main and tail verifier and repair |

### Subaru and Mitsubishi (8 Engines)

| Algorithm ID | Engine Name | Scope and Algorithm |
|---|---|---|
| `subaru_denso_sh7xxx` | Subaru Denso SH7xxx / Diesel | Auto-discovered 12-byte BE descriptors (inclusive end + diesel overrides) |
| `subaru_hitachi_m32r_kline` | Subaru Hitachi M32R K-Line | `512 KiB` `sum8`/`xor8` + `0x5AA5` balance at `0x7FFFA` |
| `subaru_hitachi_m32r_can` | Subaru Hitachi M32R CAN | `512 KiB` multi-zone BE32 `sum`/`XOR` + `0x5AA5` balance |
| `subaru_hitachi_sh7058` | Subaru Hitachi SH7058 CAN | `1 MiB` multi-zone BE32 `sum`/`XOR` + `0x5AA5A55A` balance at `0xFFFF8` |
| `subaru_hitachi_sh72543r` | Subaru Hitachi SH72543R | `2 MiB` BE16 sum = `0x5AA5` balanced at `0x1FFFFE` |
| `subaru_hitachi_tcu_m32r` | Subaru Hitachi M32R CAN TCU | BE32 at `0x8020` balance + two's complement at `0x8000` |
| `subaru_denso_tcu_sh7055` | Subaru Denso SH7055 TCU | `512 KiB` 12-area BE16 sum = `0x5AA5` at `0x7FFF4` |
| `mitsu_mh8104_tcu` | Mitsubishi MH8104 CAN CVT TCU | `512 KiB` BE32 balance at `0x81FC` targeting `0x5AA45AAB` |

Cryptographic OEM digital signatures (such as RSA / TPROT tuning protection blocks) require manufacturer private keys and cannot be regenerated offline.

---

## 2. Declarative JSON Custom Checksum Plugins

You can define custom checksum ranges in plain JSON using the `CustomChecksumPlugin` schema from `crates/kivi-checksum/src/custom_plugin.rs`. Standard JSON numbers must be written in decimal because JSON syntax does not allow `0x` hexadecimal literals.

### Top-Level Schema

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | Yes | Unique non-empty identifier (for example `"custom_ecu_v1"`) |
| `name` | string | Yes | Human-readable display name shown in the workbench UI |
| `family` | string | Yes | ECU or protocol family grouping label |
| `notes` | string or `null` | No | Optional implementation or address layout notes |
| `min_rom_size` | integer or `null` | No | Optional minimum accepted ROM size in bytes |
| `max_rom_size` | integer or `null` | No | Optional maximum accepted ROM size in bytes |
| `blocks` | array (`1..=64`) | Yes | Ordered list of `CustomChecksumBlock` entries (`MAX_CUSTOM_PLUGIN_BLOCKS = 64`) |

### Block Schema (`blocks[]`)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | Yes | Non-empty block label |
| `start` | integer | Yes | Inclusive start byte offset in decimal |
| `end_inclusive` | integer | Yes | Inclusive end byte offset (`>= start` and `< rom.len()`) |
| `store_offset` | integer | Yes | Byte offset where the checksum word is stored (`store_offset + width_bits / 8 <= rom.len()`) |
| `width_bits` | `8` \| `16` \| `32` | Yes | Checksum word size in bits |
| `endianness` | `"Big"` \| `"Little"` | Yes | Byte order used both for multi-byte word accumulation and stored word reading/writing |
| `algorithm` | enum string | Yes | One of the 10 supported algorithms below |
| `exclude_store_from_range` | boolean | No | Defaults to `true`; skips the bytes at `store_offset..store_offset + width_bytes` if they fall inside `start..=end_inclusive` |
| `target_magic` | integer or `null` | No | Optional constant target `magic` (supported only for `"Sum"` and `"TwosComplement"`), solving `(magic - sum) mod 2^width_bits` so total sum equals `target_magic` |

### Supported `algorithm` Values and Bit Widths

| `algorithm` | Allowed `width_bits` | Computation Summary |
|---|---|---|
| `"Sum"` | `8`, `16`, `32` | Wrapping word sum (`magic.wrapping_sub(sum)` if `target_magic` is set) |
| `"TwosComplement"` | `8`, `16`, `32` | `target_magic.unwrap_or(0).wrapping_sub(sum)` masked to `width_bits` |
| `"OnesComplement"` | `8`, `16`, `32` | Bitwise `!sum` masked to `width_bits` |
| `"Xor"` | `8`, `16`, `32` | Bitwise XOR across words of `width_bits` width |
| `"Crc8Maxim"` | `8` | 8-bit Maxim/Dallas CRC (polynomial `0x31` reflected) |
| `"Crc16Ccitt"` | `16` | 16-bit CCITT-FALSE CRC (`0x1021`, init `0xFFFF`) |
| `"Crc16Arc"` | `16` | 16-bit ARC / IBM CRC |
| `"Crc32Ieee"` | `32` | Standard ISO-HDLC / IEEE CRC-32 |
| `"Crc32Castagnoli"` | `32` | CRC-32C (Castagnoli) |
| `"Crc32Simos"` | `32` | Unreflected Simos CRC-32 |

---

## 3. JSON Examples

### Example A: Dual-Block Plugin (`Sum16 TwosComplement` + `CRC32 IEEE`)

Matches the built-in template returned by `sample_custom_checksum_plugin_json()` (block 1 covers bytes `0..127` with stored word at `126` / `0x7E`; block 2 covers `128..251` with little-endian CRC-32 at `252` / `0xFC`):

```json
{
  "id": "example_dual_block_v1",
  "name": "Example Dual-Block ECU Plugin",
  "family": "Generic 16-bit + CRC32",
  "notes": "Adjust start/end_inclusive/store_offset addresses for your ROM layout.",
  "min_rom_size": 256,
  "max_rom_size": null,
  "blocks": [
    {
      "name": "Calibration Sum16 Two's Complement",
      "start": 0,
      "end_inclusive": 127,
      "store_offset": 126,
      "width_bits": 16,
      "endianness": "Big",
      "algorithm": "TwosComplement",
      "exclude_store_from_range": true,
      "target_magic": null
    },
    {
      "name": "Code Footer CRC32 IEEE",
      "start": 128,
      "end_inclusive": 251,
      "store_offset": 252,
      "width_bits": 32,
      "endianness": "Little",
      "algorithm": "Crc32Ieee",
      "exclude_store_from_range": true,
      "target_magic": null
    }
  ]
}
```

### Example B: Constant-Target Balance Word (`0x5AA5` = `23205`)

When an ECU expects a 16-bit Big Endian sum across `0x00000..0x7FFFF` (`0..524287`) including a balance word at `0x7FFFE` (`524286`) to equal `0x5AA5` (`23205`), set `algorithm` to `"Sum"`, `exclude_store_from_range` to `true`, and `target_magic` to `23205`:

```json
{
  "id": "example_5aa5_balance_v1",
  "name": "512 KiB 0x5AA5 Word-Balance ECU",
  "family": "Custom 16-bit Balance",
  "notes": "Writes (23205 - sum) mod 65536 to offset 0x7FFFE.",
  "min_rom_size": 524288,
  "max_rom_size": 524288,
  "blocks": [
    {
      "name": "Main 512 KiB Word Sum Balance",
      "start": 0,
      "end_inclusive": 524287,
      "store_offset": 524286,
      "width_bits": 16,
      "endianness": "Big",
      "algorithm": "Sum",
      "exclude_store_from_range": true,
      "target_magic": 23205
    }
  ]
}
```

---

## 4. Common Validation Errors

| Error Condition | Reason and Fix |
|---|---|
| `Plugin 'id' cannot be empty` / `name` empty | Supply non-empty `id`, `name`, and `name` inside every block. |
| `Plugin defines N blocks (max 64)` | Keep `blocks.length` between `1` and `64`. |
| `end_inclusive is before start` or `exceeds ROM length` | Ensure `start <= end_inclusive < rom.len()`. |
| `width_bits must be 8, 16, or 32` | Use integer `8`, `16`, or `32` only. |
| `algorithm ... is incompatible with X-bit width` | Match CRC algorithms to their native width (`Crc8Maxim` = `8`, `Crc16*` = `16`, `Crc32*` = `32`). |
| `target_magic is only supported with Sum or TwosComplement` | Set `target_magic` to `null` for `OnesComplement`, `Xor`, and all CRC algorithms. |
