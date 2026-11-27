import React, { useId } from 'react';
import type { TutorialSceneId } from './howToUseData.ts';
import {
  renderWorkspaceScene,
  renderExplorerScene,
  renderTablesScene,
  renderScalarFlagScene,
} from './tutorialScenesWorkspace';
import {
  renderHexReverseScene,
  renderDiffChecksumScene,
  renderTelemetryEcuScene,
} from './tutorialScenesTools';

export interface TutorialCutoutArtProps {
  sceneId: TutorialSceneId;
  ariaLabel: string;
  activePin: number | null;
  onSelectPin?: (pin: number) => void;
}

/**
 * Super-lightweight, theme-aware cropped UI vector illustrations referencing
 * strictly verified KiviTuner modules in both Carbon Dark and Lab Light.
 */
export const TutorialCutoutArt: React.FC<TutorialCutoutArtProps> = ({
  sceneId,
  ariaLabel,
  activePin,
  onSelectPin,
}) => {
  const gradId = `guide-crop-${useId().replace(/:/g, '')}`;

  return (
    <svg
      viewBox="0 0 560 270"
      width={560}
      height={270}
      role="img"
      aria-label={ariaLabel}
      className="block h-auto w-full select-none"
    >
      <defs>
        <linearGradient id={gradId} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="var(--kivi-elevated)" stopOpacity="0.65" />
          <stop offset="100%" stopColor="var(--kivi-bg)" stopOpacity="0.95" />
        </linearGradient>
      </defs>

      <rect x={0} y={0} width={560} height={270} fill={`url(#${gradId})`} />

      {sceneId === 'workspace' && renderWorkspaceScene(activePin, onSelectPin)}
      {sceneId === 'explorer' && renderExplorerScene(activePin, onSelectPin)}
      {sceneId === 'tables' && renderTablesScene(activePin, onSelectPin)}
      {sceneId === 'scalar-flag' && renderScalarFlagScene(activePin, onSelectPin)}
      {sceneId === 'hex-reverse' && renderHexReverseScene(activePin, onSelectPin)}
      {sceneId === 'diff-checksum' && renderDiffChecksumScene(activePin, onSelectPin)}
      {sceneId === 'telemetry-ecu' && renderTelemetryEcuScene(activePin, onSelectPin)}
    </svg>
  );
};
