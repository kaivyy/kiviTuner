import React, { useEffect, useMemo, useRef, useState } from 'react';
import { BookOpen, ChevronLeft, ChevronRight, Lightbulb, Search, X } from 'lucide-react';
import { FloatingWindow, useFloatingWindowContext } from '../layout/FloatingWindow';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';
import {
  TUTORIAL_CHAPTERS,
  filterTutorialChapters,
  resolveAdjacentChapterId,
  type TutorialChapter,
  type TutorialSceneId,
} from './howToUseData.ts';
import { TutorialCutoutArt } from './TutorialCutoutArt';

export interface HowToUseModalProps {
  isOpen: boolean;
  onClose: () => void;
  standalone?: boolean;
}

const HowToUseBody: React.FC<{
  searchQuery: string;
  onSearchChange: (q: string) => void;
  filteredChapters: TutorialChapter[];
  activeChapter: TutorialChapter | null;
  onSelectChapter: (id: TutorialSceneId) => void;
  activePin: number | null;
  onTogglePin: (pin: number) => void;
  contentRef: React.RefObject<HTMLDivElement>;
}> = ({
  searchQuery,
  onSearchChange,
  filteredChapters,
  activeChapter,
  onSelectChapter,
  activePin,
  onTogglePin,
  contentRef,
}) => {
  const { width } = useFloatingWindowContext();
  const isWide = width >= 660;
  const searchInputRef = useRef<HTMLInputElement>(null);

  const searchBox = (
    <div className="relative">
      <label htmlFor="guide-search" className="sr-only">
        Cari topik panduan KiviTuner
      </label>
      <Search className="pointer-events-none absolute left-2.5 top-2.5 h-3.5 w-3.5 text-kiviMuted" aria-hidden="true" />
      <input
        ref={searchInputRef}
        id="guide-search"
        data-testid="guide-search-input"
        type="text"
        value={searchQuery}
        onChange={(e) => onSearchChange(e.target.value)}
        placeholder="Cari topik (ELM327, Hex, 1x1)..."
        className="w-full rounded-md border border-kiviBorderStrong bg-kiviBg py-1.5 pl-8 pr-7 text-[11px] text-kiviInk placeholder:text-kiviMuted focus:border-kiviAccent"
      />
      {searchQuery && (
        <button
          type="button"
          onClick={() => {
            onSearchChange('');
            searchInputRef.current?.focus();
          }}
          aria-label="Hapus pencarian panduan"
          className="absolute right-2 top-2 text-kiviMuted hover:text-kiviInk"
        >
          <X className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      )}
    </div>
  );

  const chapterButtons = filteredChapters.map((ch) => {
    const selected = activeChapter?.id === ch.id;
    return (
      <button
        key={ch.id}
        type="button"
        data-testid={`guide-chapter-${ch.id}`}
        aria-current={selected ? 'step' : undefined}
        onClick={() => onSelectChapter(ch.id)}
        className={`flex items-center gap-2 rounded-lg border text-left text-[11px] transition-colors ${
          isWide
            ? 'w-full px-2.5 py-2'
            : 'min-h-[44px] shrink-0 whitespace-nowrap px-2.5 py-1.5 sm:min-h-[34px]'
        } ${
          selected
            ? 'border-kiviAccent bg-kiviElevated font-semibold text-kiviInk'
            : isWide
              ? 'border-transparent bg-transparent text-kiviSecondary hover:border-kiviBorder hover:bg-kiviSurface hover:text-kiviInk'
              : 'border-kiviBorder bg-kiviSurface text-kiviSecondary hover:text-kiviInk'
        }`}
      >
        <span
          className={`inline-flex shrink-0 items-center justify-center rounded text-[10px] font-bold tabular-nums ${
            isWide ? 'h-5 w-5' : 'h-4 w-4'
          } ${
            selected
              ? 'bg-kiviAccent text-kiviAccentFg'
              : 'border border-kiviBorder bg-kiviSurface text-kiviMuted'
          }`}
        >
          {ch.index}
        </span>
        <span className="min-w-0 truncate">{ch.shortTitle}</span>
      </button>
    );
  });

  return (
    <div
      data-testid="how-to-use-body"
      className={`flex min-h-0 flex-1 overflow-hidden ${isWide ? 'flex-row' : 'flex-col'}`}
    >
      {isWide ? (
        <aside className="flex w-[228px] shrink-0 flex-col border-r border-kiviBorder bg-kiviBg">
          <div className="shrink-0 border-b border-kiviBorder p-3">{searchBox}</div>
          <nav aria-label="Daftar bab tutorial" className="flex-1 space-y-1 overflow-y-auto p-2.5">
            {chapterButtons}
          </nav>
        </aside>
      ) : (
        <div className="shrink-0 space-y-2 border-b border-kiviBorder bg-kiviBg p-3">
          {searchBox}
          <nav aria-label="Daftar bab tutorial" className="no-scrollbar flex items-center gap-1.5 overflow-x-auto pb-0.5">
            {chapterButtons}
          </nav>
        </div>
      )}

      <div ref={contentRef} className="min-w-0 flex-1 overflow-y-auto bg-kiviSurface p-4 sm:p-5">
        {!activeChapter ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 py-10 text-center">
            <BookOpen className="h-7 w-7 text-kiviMuted" aria-hidden="true" />
            <div className="space-y-1">
              <p className="text-xs font-semibold text-kiviInk">Topik &quot;{searchQuery}&quot; tidak ditemukan</p>
              <p className="text-[11px] text-kiviMuted">
                Coba kata kunci lain seperti ROM, Hex, 1x1, Checksum, atau ELM327.
              </p>
            </div>
            <button
              type="button"
              data-testid="guide-reset-search"
              onClick={() => onSearchChange('')}
              className="min-h-[40px] rounded-md border border-kiviBorderStrong bg-kiviElevated px-3 py-1.5 text-[11px] font-semibold text-kiviInk hover:border-kiviAccent sm:min-h-0"
            >
              Reset Pencarian
            </button>
          </div>
        ) : (
          <div className="mx-auto max-w-2xl space-y-4">
            <header className="space-y-1">
              <div className="flex flex-wrap items-center justify-between gap-2 text-[10px] text-kiviMuted">
                <span className="font-semibold uppercase tracking-wider text-kiviAccent tabular-nums">
                  Langkah {activeChapter.index} dari {TUTORIAL_CHAPTERS.length}
                </span>
                <span className="truncate rounded border border-kiviBorder bg-kiviBg px-2 py-0.5 text-kiviSecondary">
                  {activeChapter.menuPath}
                </span>
              </div>
              <h3 className="text-sm font-bold text-kiviInk sm:text-base">{activeChapter.title}</h3>
              <p className="text-[11px] leading-relaxed text-kiviSecondary">{activeChapter.subtitle}</p>
            </header>

            <figure className="overflow-hidden rounded-xl border border-kiviBorder bg-kiviBg">
              <div className="aspect-[560/270] w-full">
                <TutorialCutoutArt
                  sceneId={activeChapter.id}
                  ariaLabel={activeChapter.figureAriaLabel}
                  activePin={activePin}
                  onSelectPin={onTogglePin}
                />
              </div>
              <figcaption className="border-t border-kiviBorder bg-kiviElevated/50 px-3 py-1.5 text-[10px] text-kiviMuted">
                {activeChapter.figureCaption}
              </figcaption>
            </figure>

            <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
              {activeChapter.callouts.map((c) => {
                const pressed = activePin === c.pin;
                return (
                  <button
                    key={c.pin}
                    type="button"
                    data-testid={`guide-callout-pin-${c.pin}`}
                    aria-pressed={pressed}
                    onClick={() => onTogglePin(c.pin)}
                    className={`flex min-h-[44px] flex-col items-start gap-1 rounded-lg border p-2.5 text-left transition-colors ${
                      pressed
                        ? 'border-kiviAccent bg-kiviElevated'
                        : 'border-kiviBorder bg-kiviBg hover:border-kiviBorderStrong'
                    }`}
                  >
                    <div className="flex w-full items-center gap-1.5">
                      <span
                        className={`inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-[10px] font-bold tabular-nums ${
                          pressed
                            ? 'bg-kiviAccent text-kiviAccentFg'
                            : 'border border-kiviBorderStrong bg-kiviSurface text-kiviInk'
                        }`}
                      >
                        {c.pin}
                      </span>
                      <span className="min-w-0 text-[11px] font-semibold leading-tight text-kiviInk">{c.label}</span>
                    </div>
                    <p className="text-[10px] leading-snug text-kiviSecondary">{c.detail}</p>
                  </button>
                );
              })}
            </div>

            <section className="space-y-2 rounded-lg border border-kiviBorder bg-kiviBg p-3.5">
              <h4 className="text-[11px] font-bold uppercase tracking-wider text-kiviSecondary">
                Urutan Penggunaan
              </h4>
              <ol className="space-y-2">
                {activeChapter.steps.map((step, i) => (
                  <li key={i} className="flex items-start gap-2.5 text-[11px] leading-relaxed text-kiviInk">
                    <span className="mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center rounded bg-kiviElevated text-[10px] font-bold text-kiviSecondary tabular-nums">
                      {i + 1}
                    </span>
                    <span>{step}</span>
                  </li>
                ))}
              </ol>
            </section>

            {activeChapter.shortcuts.length > 0 && (
              <div className="flex flex-wrap items-center gap-2 rounded-lg border border-kiviBorder bg-kiviBg px-3 py-2">
                <span className="text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">Pintasan:</span>
                {activeChapter.shortcuts.map((sc) => (
                  <div key={sc.action} className="inline-flex items-center gap-1.5 rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-[10px]">
                    <span className="inline-flex items-center gap-0.5">
                      {sc.keys.map((k, kIdx) => (
                        <React.Fragment key={kIdx}>
                          <kbd className="rounded border border-kiviBorderStrong bg-kiviElevated px-1 font-mono text-[10px] font-bold text-kiviInk">
                            {k}
                          </kbd>
                          {kIdx < sc.keys.length - 1 && <span className="text-kiviMuted">+</span>}
                        </React.Fragment>
                      ))}
                    </span>
                    <span className="text-kiviSecondary">{sc.action}</span>
                  </div>
                ))}
              </div>
            )}

            <div className="flex items-start gap-2 rounded-lg border border-kiviBorder bg-kiviElevated/60 px-3 py-2.5 text-[11px] text-kiviSecondary">
              <Lightbulb className="mt-0.5 h-3.5 w-3.5 shrink-0 text-kiviWarn" aria-hidden="true" />
              <p className="leading-relaxed">
                <span className="font-semibold text-kiviInk">Catatan Praktis: </span>
                {activeChapter.tip}
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

const HowToUseFooter: React.FC<{
  activeChapter: TutorialChapter | null;
  prevId: TutorialSceneId | null;
  nextId: TutorialSceneId | null;
  onSelectChapter: (id: TutorialSceneId) => void;
  onClose: () => void;
}> = ({ activeChapter, prevId, nextId, onSelectChapter, onClose }) => {
  const { width } = useFloatingWindowContext();
  return (
    <div className="flex shrink-0 items-center justify-between gap-2 border-t border-kiviBorder bg-kiviBg px-4 py-2 text-[11px] text-kiviMuted">
      <div className="flex items-center gap-3 tabular-nums">
        <span>Bab {activeChapter ? activeChapter.index : 0} / {TUTORIAL_CHAPTERS.length}</span>
        {width >= 520 && (
          <span className="inline-flex items-center gap-1.5">
            <kbd className="rounded border border-kiviBorderStrong bg-kiviElevated px-1.5 py-0.5 text-[10px] text-kiviSecondary">Esc</kbd>
            <span>Tutup</span>
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        <button
          type="button"
          data-testid="guide-prev-btn"
          disabled={!prevId}
          onClick={() => prevId && onSelectChapter(prevId)}
          className="inline-flex min-h-[44px] items-center gap-1 rounded-md border border-kiviBorderStrong bg-kiviElevated px-3 py-1 text-[11px] font-medium text-kiviInk transition-colors hover:border-kiviAccent disabled:opacity-40 sm:min-h-[30px]"
        >
          <ChevronLeft className="h-3.5 w-3.5" aria-hidden="true" />
          <span>Sebelumnya</span>
        </button>
        <button
          type="button"
          data-testid="guide-next-btn"
          onClick={() => (nextId ? onSelectChapter(nextId) : onClose())}
          className="inline-flex min-h-[44px] items-center gap-1 rounded-md border border-kiviAccent bg-kiviAccent px-3 py-1 text-[11px] font-semibold text-kiviAccentFg transition-opacity hover:opacity-90 sm:min-h-[30px]"
        >
          <span>{nextId ? 'Berikutnya' : 'Selesai'}</span>
          {nextId && <ChevronRight className="h-3.5 w-3.5" aria-hidden="true" />}
        </button>
      </div>
    </div>
  );
};

export const HowToUseModal: React.FC<HowToUseModalProps> = ({ isOpen, onClose, standalone = false }) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedId, setSelectedId] = useState<TutorialSceneId>('workspace');
  const [activePin, setActivePin] = useState<number | null>(1);
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isOpen) {
      setSearchQuery('');
      setSelectedId('workspace');
      setActivePin(1);
    }
  }, [isOpen]);

  const filteredChapters = useMemo(() => filterTutorialChapters(TUTORIAL_CHAPTERS, searchQuery), [searchQuery]);
  const activeChapter = useMemo(
    () => (filteredChapters.length === 0 ? null : filteredChapters.find((c) => c.id === selectedId) ?? filteredChapters[0]),
    [filteredChapters, selectedId]
  );

  useEffect(() => {
    contentRef.current?.scrollTo({ top: 0 });
  }, [activeChapter?.id]);

  const navList = filteredChapters.length > 0 ? filteredChapters : TUTORIAL_CHAPTERS;
  const prevId = activeChapter ? resolveAdjacentChapterId(navList, activeChapter.id, -1) : null;
  const nextId = activeChapter ? resolveAdjacentChapterId(navList, activeChapter.id, 1) : null;
  const spec = TOOL_WINDOW_SPECS.guide;

  return (
    <FloatingWindow
      windowId="guide"
      isOpen={isOpen}
      onClose={onClose}
      title="How to Use kiviTuner"
      titleId="how-to-use-modal-title"
      badge="Panduan Interaktif"
      icon={<BookOpen className="h-4 w-4" aria-hidden="true" />}
      titlebarTone="sidebar"
      defaultWidth={spec.width}
      defaultHeight={spec.height}
      minWidth={spec.minWidth}
      minHeight={spec.minHeight}
      dock={spec.dock}
      standalone={standalone}
      popoutToolId="guide"
      closeLabel="Close How to Use window"
      footer={
        <HowToUseFooter
          activeChapter={activeChapter}
          prevId={prevId}
          nextId={nextId}
          onSelectChapter={(id) => {
            setSelectedId(id);
            setActivePin(1);
          }}
          onClose={onClose}
        />
      }
    >
      <HowToUseBody
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        filteredChapters={filteredChapters}
        activeChapter={activeChapter}
        onSelectChapter={(id) => {
          setSelectedId(id);
          setActivePin(1);
        }}
        activePin={activePin}
        onTogglePin={(pin) => setActivePin((cur) => (cur === pin ? null : pin))}
        contentRef={contentRef}
      />
    </FloatingWindow>
  );
};
