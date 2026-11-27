import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  TUTORIAL_CHAPTERS,
  filterTutorialChapters,
  resolveAdjacentChapterId,
} from './howToUseData.ts';

const EMOJI_RE = /[\u{1F300}-\u{1FAFF}\u{2600}-\u{27BF}]/u;
const PROPRIETARY_BRAND_RE =
  /\b(winols|tunerpro|tunerstudio|pcmflash|romraider|ecuflash|haltech|motec|mmcflash|alientech|hptuners|efilive|dimsport|swiftec|bflash|cmdflash|bootmod3)\b/i;

test('howToUseData exposes 7 ordered chapters with clean copy and sequential callouts', () => {
  assert.equal(TUTORIAL_CHAPTERS.length, 7);

  const seenIds = new Set<string>();
  TUTORIAL_CHAPTERS.forEach((chapter, idx) => {
    assert.equal(chapter.index, idx + 1);
    assert.ok(!seenIds.has(chapter.id), `duplicate chapter id: ${chapter.id}`);
    seenIds.add(chapter.id);

    assert.ok(chapter.shortTitle.trim().length > 0);
    assert.ok(chapter.title.trim().length > 0);
    assert.ok(chapter.subtitle.trim().length > 0);
    assert.ok(chapter.menuPath.trim().length > 0);
    assert.ok(chapter.figureCaption.trim().length > 0);
    assert.ok(chapter.figureAriaLabel.trim().length > 0);
    assert.ok(chapter.steps.length >= 3, `${chapter.id} should document at least 3 concrete steps`);
    assert.ok(chapter.callouts.length >= 3, `${chapter.id} should document at least 3 cutout pins`);

    chapter.callouts.forEach((callout, pinIdx) => {
      assert.equal(callout.pin, pinIdx + 1, `${chapter.id} callout pins must be 1..N sequential`);
      assert.ok(callout.label.trim().length > 0);
      assert.ok(callout.detail.trim().length > 0);
    });

    const allStrings = [
      chapter.shortTitle,
      chapter.title,
      chapter.subtitle,
      chapter.menuPath,
      chapter.figureCaption,
      chapter.figureAriaLabel,
      chapter.tip,
      ...chapter.steps,
      ...chapter.callouts.flatMap((c) => [c.label, c.detail]),
      ...chapter.shortcuts.map((s) => s.action),
    ];
    for (const text of [...allStrings, ...chapter.keywords]) {
      assert.ok(!text.includes('\u2014'), `em dash forbidden in copy: "${text}"`);
      assert.ok(!EMOJI_RE.test(text), `decorative emoji forbidden in copy: "${text}"`);
      assert.ok(
        !PROPRIETARY_BRAND_RE.test(text),
        `proprietary brand name forbidden in tutorial copy: "${text}"`
      );
    }
  });
});

test('filterTutorialChapters matches real workflows and handles empty/missing queries', () => {
  assert.equal(filterTutorialChapters(TUTORIAL_CHAPTERS, '').length, 7);
  assert.equal(filterTutorialChapters(TUTORIAL_CHAPTERS, '   ').length, 7);

  const elm = filterTutorialChapters(TUTORIAL_CHAPTERS, 'ELM327');
  assert.ok(elm.some((c) => c.id === 'telemetry-ecu'));

  const bitmask = filterTutorialChapters(TUTORIAL_CHAPTERS, 'bitmask');
  assert.ok(bitmask.some((c) => c.id === 'scalar-flag'));

  const scalar1x1 = filterTutorialChapters(TUTORIAL_CHAPTERS, '1x1');
  assert.ok(scalar1x1.some((c) => c.id === 'explorer'));
  assert.ok(scalar1x1.some((c) => c.id === 'scalar-flag'));

  const checksum = filterTutorialChapters(TUTORIAL_CHAPTERS, 'checksum');
  assert.ok(checksum.some((c) => c.id === 'diff-checksum'));

  const hexdump = filterTutorialChapters(TUTORIAL_CHAPTERS, 'hexdump');
  assert.ok(hexdump.some((c) => c.id === 'hex-reverse'));

  assert.equal(filterTutorialChapters(TUTORIAL_CHAPTERS, 'nonexistent_token_xyz').length, 0);
});

test('resolveAdjacentChapterId navigates smoothly and stops cleanly at boundaries', () => {
  assert.equal(resolveAdjacentChapterId(TUTORIAL_CHAPTERS, 'workspace', -1), null);
  assert.equal(resolveAdjacentChapterId(TUTORIAL_CHAPTERS, 'workspace', 1), 'explorer');
  assert.equal(resolveAdjacentChapterId(TUTORIAL_CHAPTERS, 'hex-reverse', -1), 'scalar-flag');
  assert.equal(resolveAdjacentChapterId(TUTORIAL_CHAPTERS, 'hex-reverse', 1), 'diff-checksum');
  assert.equal(resolveAdjacentChapterId(TUTORIAL_CHAPTERS, 'telemetry-ecu', 1), null);
});
