/**
 * Pure Change Ledger model clustering modified ROM offsets vs original baseline.
 */

export interface DeltaCluster {
  start: number;
  end: number;
  changedBytes: number;
  firstOld: number;
  firstNew: number;
}

export interface RomDeltaLedger {
  totalChangedBytes: number;
  totalClusterCount: number;
  clusters: DeltaCluster[];
  truncated: boolean;
}

const DEFAULT_CLUSTER_GAP = 4;

/**
 * Computes contiguous/near-contiguous clusters (`gap <= 4` bytes) where `current !== original`.
 */
export function computeRomDeltaLedger(
  current: Uint8Array | null,
  original: Uint8Array | null,
  maxClusters = 12
): RomDeltaLedger {
  if (!current || !original || current.length === 0 || original.length === 0) {
    return {
      totalChangedBytes: 0,
      totalClusterCount: 0,
      clusters: [],
      truncated: false,
    };
  }

  const len = Math.min(current.length, original.length);
  const clusters: DeltaCluster[] = [];
  let totalChangedBytes = 0;
  let totalClusterCount = 0;
  let active: DeltaCluster | null = null;

  for (let i = 0; i < len; i++) {
    const cur = current[i];
    const orig = original[i];
    if (cur === orig) continue;

    totalChangedBytes++;
    if (active && i - active.end <= DEFAULT_CLUSTER_GAP) {
      active.end = i;
      active.changedBytes++;
    } else {
      totalClusterCount++;
      active = {
        start: i,
        end: i,
        changedBytes: 1,
        firstOld: orig,
        firstNew: cur,
      };
      if (clusters.length < maxClusters) {
        clusters.push(active);
      }
    }
  }

  return {
    totalChangedBytes,
    totalClusterCount,
    clusters,
    truncated: totalClusterCount > clusters.length,
  };
}
