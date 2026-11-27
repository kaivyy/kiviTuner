import type { BinaryHashes, MapDef } from '../types/kivi.ts';
import { resolveDisplayAxisLabels } from './matrixRom.ts';

export function downloadTextFile(
  content: string,
  filename: string,
  mime = 'text/plain;charset=utf-8'
): void {
  const blob = new Blob([content], { type: mime });
  downloadBlob(blob, filename);
}

export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

export async function saveRomBytesToDisk(
  bytes: Uint8Array,
  suggestedName: string,
  forcePicker = false
): Promise<{ saved: boolean; filename: string; usedPicker: boolean }> {
  const win = window as unknown as {
    showSaveFilePicker?: (opts: {
      suggestedName: string;
      types: Array<{ description: string; accept: Record<string, string[]> }>;
    }) => Promise<{
      name?: string;
      createWritable: () => Promise<{ write: (data: Blob) => Promise<void>; close: () => Promise<void> }>;
    }>;
  };
  const bufferCopy = new Uint8Array(bytes.byteLength);
  bufferCopy.set(bytes);
  const blob = new Blob([bufferCopy], { type: 'application/octet-stream' });

  if (forcePicker && typeof win.showSaveFilePicker === 'function') {
    try {
      const handle = await win.showSaveFilePicker({
        suggestedName,
        types: [
          {
            description: 'ECU Binary Image',
            accept: { 'application/octet-stream': ['.bin', '.rom', '.ori', '.mod'] },
          },
        ],
      });
      const writable = await handle.createWritable();
      await writable.write(blob);
      await writable.close();
      return { saved: true, filename: handle.name || suggestedName, usedPicker: true };
    } catch (err) {
      if ((err as { name?: string })?.name === 'AbortError') {
        return { saved: false, filename: suggestedName, usedPicker: true };
      }
    }
  }

  downloadBlob(blob, suggestedName);
  return { saved: true, filename: suggestedName, usedPicker: false };
}

function escapeCsvCell(val: string | number): string {
  const s = String(val);
  return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

export function serializeMapToCsv(
  map: MapDef,
  matrix: number[][],
  baseMatrix?: number[][] | null
): string {
  const decimals = map.scaling.decimals ?? 2;
  const { xLabels, yLabels, xAxisName, yAxisName } = resolveDisplayAxisLabels(map);
  const lines: string[] = [];

  lines.push(`# Map,${escapeCsvCell(map.name)},Address,0x${map.address.toString(16).toUpperCase()},Unit,${escapeCsvCell(map.scaling.unit || 'raw')}`);
  lines.push(
    [escapeCsvCell(`${yAxisName} \\ ${xAxisName}`), ...xLabels.map(escapeCsvCell)].join(
      ','
    )
  );
  matrix.forEach((row, r) => {
    const yVal = yLabels[r] ?? r;
    const cells = row.map((v) => v.toFixed(decimals));
    lines.push([escapeCsvCell(yVal), ...cells].join(','));
  });

  if (baseMatrix && baseMatrix.length === matrix.length) {
    lines.push('');
    lines.push('# Stock Deltas (Live - Stock)');
    matrix.forEach((row, r) => {
      const yVal = yLabels[r] ?? r;
      const deltas = row.map((v, c) => (v - (baseMatrix[r]?.[c] ?? v)).toFixed(decimals));
      lines.push([escapeCsvCell(yVal), ...deltas].join(','));
    });
  }
  return lines.join('\r\n');
}

export function buildCalibrationReportMarkdown(opts: {
  romLabel: string;
  romSize: number;
  hashes: BinaryHashes | null;
  checksumStatus: 'valid' | 'invalid' | null;
  checksumAlgorithmName: string | null;
  maps: MapDef[];
  selectedMap?: MapDef;
  activeMatrix?: number[][] | null;
  baseMatrix?: number[][] | null;
}): string {
  const lines: string[] = [
    `# kiviTuner Calibration Report`,
    ``,
    `- Generated: ${new Date().toISOString()}`,
    `- Active ROM: ${opts.romLabel}`,
    `- ROM Size: ${opts.romSize.toLocaleString()} bytes`,
    `- SHA-256: ${opts.hashes?.sha256 ?? 'unavailable'}`,
    `- MD5: ${opts.hashes?.md5 ?? 'unavailable'}`,
    `- Checksum: ${opts.checksumStatus ?? 'unverified'} (${opts.checksumAlgorithmName ?? 'auto'})`,
    `- Loaded Maps: ${opts.maps.length}`,
    ``,
    `## Defined Calibration Tables`,
    ``,
    `| Name | Category | Address | Dims | DType | Endian | Equation |`,
    `|---|---|---|---|---|---|---|`,
    ...opts.maps.map(
      (m) =>
        `| ${m.name} | ${m.category} | 0x${m.address.toString(16).toUpperCase()} | ${m.rows}x${m.cols} | ${m.dtype} | ${m.endian} | \`${m.scaling.equation}\` |`
    ),
  ];

  if (opts.selectedMap && opts.activeMatrix && opts.baseMatrix) {
    const changed: string[] = [];
    const decimals = opts.selectedMap.scaling.decimals ?? 2;
    opts.activeMatrix.forEach((row, r) => {
      row.forEach((v, c) => {
        const base = opts.baseMatrix?.[r]?.[c];
        if (base !== undefined && Math.abs(v - base) > 1e-6) {
          changed.push(`- (${r}, ${c}): ${base.toFixed(decimals)} -> ${v.toFixed(decimals)}`);
        }
      });
    });
    lines.push('', `## Active Map Deltas (${opts.selectedMap.name})`, '');
    lines.push(changed.length > 0 ? changed.join('\n') : 'No modified cells vs stock.');
  }

  return lines.join('\n');
}

function escapeXml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export function exportDefinitions(
  maps: MapDef[],
  format: 'xdf' | 'json' | 'ini' | 'kp',
  romSize = 0
): { content: string; filename: string; mime: string } {
  if (format === 'json') {
    return {
      content: JSON.stringify({ version: 1, maps }, null, 2),
      filename: 'definitions.kivi.json',
      mime: 'application/json',
    };
  }

  if (format === 'ini') {
    const lines = [
      '; Exported by kiviTuner',
      '[Constants]',
      ...maps.map((m) => {
        const typeToken =
          m.dtype === 'f32'
            ? 'F32'
            : m.dtype === 'u32'
              ? 'U32'
              : m.dtype === 'i32'
                ? 'S32'
                : m.dtype === 'i16'
                  ? 'S16'
                  : m.dtype === 'u16'
                    ? 'U16'
                    : m.dtype === 'i8'
                      ? 'S08'
                      : 'U08';
        const safeName = m.name.replace(/[^A-Za-z0-9_]/g, '_');
        return `  ${safeName} = array, ${typeToken}, ${m.address}, [${m.rows}x${m.cols}], "${m.scaling.unit || ''}", 1.0, 0.0, 0, 65535, ${m.scaling.decimals ?? 1}`;
      }),
    ];
    return {
      content: lines.join('\n'),
      filename: 'kivituner_export.ini',
      mime: 'text/plain;charset=utf-8',
    };
  }

  if (format === 'kp') {
    const header = 'Name;Category;Address;Rows;Cols;Bits;Signed;Float;LSBFirst;Factor;Offset;Unit';
    const rows = maps.map((m) => {
      const bits = m.dtype === 'u8' || m.dtype === 'i8' ? 8 : m.dtype === 'u16' || m.dtype === 'i16' ? 16 : 32;
      const isSigned = m.dtype.startsWith('i') ? 1 : 0;
      const isFloat = m.dtype === 'f32' ? 1 : 0;
      const lsb = m.endian === 'little' ? 1 : 0;
      return `${m.name.replace(/;/g, ',')};${m.category.replace(/;/g, ',')};0x${m.address.toString(16).toUpperCase()};${m.rows};${m.cols};${bits};${isSigned};${isFloat};${lsb};1;0;${m.scaling.unit || ''}`;
    });
    return {
      content: [header, ...rows].join('\r\n'),
      filename: 'kivituner_maps.csv',
      mime: 'text/csv;charset=utf-8',
    };
  }

  const tablesXml = maps
    .map((m) => {
      const bits = m.dtype === 'u8' || m.dtype === 'i8' ? 8 : m.dtype === 'u16' || m.dtype === 'i16' ? 16 : 32;
      const flags =
        (m.dtype.startsWith('i') ? 0x01 : 0) |
        (m.endian === 'little' ? 0x02 : 0) |
        (m.column_major ? 0x04 : 0) |
        (m.dtype === 'f32' ? 0x100 : 0);
      return [
        `  <XDFTABLE uniqueid="0x${m.address.toString(16)}" flags="0x0">`,
        `    <title>${escapeXml(m.name)}</title>`,
        `    <XDFAXIS id="z">`,
        `      <EMBEDDEDDATA mmedtypeflags="0x${flags.toString(16)}" mmedaddress="0x${m.address.toString(16)}" mmedelementsizebits="${bits}" mmedrowcount="${m.rows}" mmedcolcount="${m.cols}" />`,
        `      <units>${escapeXml(m.scaling.unit || 'raw')}</units>`,
        `      <decimalpl>${m.scaling.decimals ?? 2}</decimalpl>`,
        `      <MATH equation="${escapeXml(m.scaling.equation || 'X')}"><VAR id="X" /></MATH>`,
        `    </XDFAXIS>`,
        `  </XDFTABLE>`,
      ].join('\n');
    })
    .join('\n');

  const xdf = [
    `<!-- Written by kiviTuner -->`,
    `<XDFFORMAT version="1.70">`,
    `  <XDFHEADER>`,
    `    <deftitle>kiviTuner Export</deftitle>`,
    `    <BASEOFFSET offset="0" subtract="0" />`,
    `    <DEFAULTS datasizeinbits="8" sigdigits="2" outputtype="1" signed="0" lsbfirst="0" float="0" />`,
    `    <REGION type="0xFFFFFFFF" startaddress="0x0" size="0x${Math.max(romSize, 0x10000).toString(16)}" regionflags="0x0" name="Binary File" desc="Exported from kiviTuner" />`,
    `  </XDFHEADER>`,
    tablesXml,
    `</XDFFORMAT>`,
  ].join('\n');

  return {
    content: xdf,
    filename: 'kivituner_export.xdf',
    mime: 'application/xml;charset=utf-8',
  };
}
