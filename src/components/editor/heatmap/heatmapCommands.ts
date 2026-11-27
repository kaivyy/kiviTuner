export interface HeatmapTableHandle {
  copySelection: () => void;
  pasteSelection: () => void;
  pasteSpecial: (mode: 'add' | 'multiply' | 'hex') => void;
  selectAll: () => void;
  invertSelection: () => void;
  clearSelection: () => void;
  openMathDialog: () => void;
  applySmooth3x3: () => void;
  applyInterpolate: (mode: 'bilinear' | 'horizontal' | 'vertical') => void;
  revertSelectionToOriginal: () => void;
}
