/**
 * Data contracts and TypeScript interfaces matching kivi-core, kivi-defs, and kivi-project.
 */

export type Endian = 'big' | 'little';

export type DType = 'u8' | 'i8' | 'u16' | 'i16' | 'u32' | 'i32' | 'f32';

export interface BinaryHashes {
  md5: string;
  sha1: string;
  sha256: string;
}

export interface Scaling {
  equation: string;
  unit: string;
  decimals: number;
}

export interface AxisDef {
  address: number;
  length: number;
  dtype: DType;
  endian: Endian;
  scaling: Scaling;
  values?: number[];
}

export interface MapDef {
  id: string;
  name: string;
  category: string;
  address: number;
  rows: number;
  cols: number;
  dtype: DType;
  endian: Endian;
  column_major?: boolean;
  major_stride_bits?: number;
  minor_stride_bits?: number;
  scaling: Scaling;
  x_axis?: AxisDef | null;
  y_axis?: AxisDef | null;
}

export interface ConstantDef {
  id: string;
  name: string;
  category: string;
  address: number;
  dtype: DType;
  endian: Endian;
  scaling: Scaling;
}

export interface FlagDef {
  id: string;
  name: string;
  category: string;
  address: number;
  dtype: DType;
  mask: number;
}

export interface Definitions {
  maps: MapDef[];
  constants: ConstantDef[];
  flags: FlagDef[];
}

export interface CellDelta {
  address: number;
  old_raw: number;
  new_raw: number;
}

export interface CommitRecord {
  id: number;
  timestamp: number;
  message: string;
  deltas: CellDelta[];
}

export type DiffKind =
  | { type: 'Scalar' }
  | { type: 'Axis1D'; data: { length: number } }
  | { type: 'Table2D'; data: { suggested_rows: number; suggested_cols: number } }
  | { type: 'ChecksumOrRawBlock' };

export interface DiffRegion {
  id: string;
  start_addr: number;
  end_addr: number;
  byte_len: number;
  changed_byte_count: number;
  max_abs_delta: number;
  mean_delta: number;
  inferred_dtype: DType;
  inferred_endian: Endian;
  classification: DiffKind;
}

export interface DetectedMapCandidate {
  id: string;
  name_hint: string;
  category_hint: string;
  start_addr: number;
  end_addr: number;
  rows: number;
  cols: number;
  dtype: DType;
  endian: Endian;
  confidence: number;
  x_axis_addr?: number | null;
  y_axis_addr?: number | null;
}

