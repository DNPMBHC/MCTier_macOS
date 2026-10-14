import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

// 单独加载 voiceMode.ts（纯状态机，无 DOM 依赖），验证三种语音模式的传输门控逻辑。
const code = ts.transpileModule(fs.readFileSync('src/services/voice/voiceMode.ts', 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const context = vm.createContext({ exports: {}, console });
vm.runInContext(code, context);
const { voiceMode, normalizeVoiceMode } = context.exports;

test('open mode always transmits regardless of ptt/vad state', () => {
  voiceMode.setMode('open');
  assert.equal(voiceMode.transmitting(), true);
  voiceMode.setPttHeld(false);
  voiceMode.setSpeaking(false);
  assert.equal(voiceMode.transmitting(), true, '常开模式与按键/说话状态无关');
});

test('ptt mode transmits only while the key is held', () => {
  voiceMode.setMode('ptt');
  assert.equal(voiceMode.transmitting(), false, '切到 PTT 应清零按住状态，默认不发声');
  voiceMode.setPttHeld(true);
  assert.equal(voiceMode.transmitting(), true);
  voiceMode.setPttHeld(false);
  assert.equal(voiceMode.transmitting(), false);
});

test('vad mode transmits only while speaking is detected', () => {
  voiceMode.setMode('vad');
  assert.equal(voiceMode.transmitting(), false);
  voiceMode.setSpeaking(true);
  assert.equal(voiceMode.transmitting(), true);
  voiceMode.setSpeaking(false);
  assert.equal(voiceMode.transmitting(), false);
});

test('switching modes clears stale ptt/vad transient state', () => {
  voiceMode.setMode('ptt');
  voiceMode.setPttHeld(true);
  assert.equal(voiceMode.transmitting(), true);
  // 切到 VAD：之前按住的 PTT 不该泄漏成「正在说话」
  voiceMode.setMode('vad');
  assert.equal(voiceMode.transmitting(), false);
  // 切回常开立即恢复发声
  voiceMode.setMode('open');
  assert.equal(voiceMode.transmitting(), true);
});

test('subscribers are notified only on effective transmit changes', () => {
  voiceMode.setMode('ptt');
  let hits = 0;
  const off = voiceMode.subscribe(() => { hits += 1; });
  voiceMode.setPttHeld(true);  // ptt 模式下按键变化 → 通知
  assert.equal(hits, 1);
  voiceMode.setSpeaking(true); // ptt 模式下说话状态变化 → 不该通知
  assert.equal(hits, 1, 'PTT 模式不该因 VAD 状态变化而通知');
  off();
  voiceMode.setPttHeld(false);
  assert.equal(hits, 1, '退订后不再收到通知');
});

test('normalizeVoiceMode falls back to open for unknown values', () => {
  assert.equal(normalizeVoiceMode('ptt'), 'ptt');
  assert.equal(normalizeVoiceMode('vad'), 'vad');
  assert.equal(normalizeVoiceMode('open'), 'open');
  assert.equal(normalizeVoiceMode(undefined), 'open');
  assert.equal(normalizeVoiceMode('garbage'), 'open');
  assert.equal(normalizeVoiceMode(42), 'open');
});
