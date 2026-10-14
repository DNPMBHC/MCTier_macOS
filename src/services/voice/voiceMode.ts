/**
 * 语音模式（传输门控）。
 *
 * 控制「麦克风已开」时音频是否真的向外发送，三种模式：
 * - `open` 常开：麦克风开着就一直传（原有行为，默认）。
 * - `ptt`  按键说话：仅在按住 PTT 键期间传；松开即停。按键来自后端全局快捷键
 *   （`ptt-pressed` / `ptt-released` 事件），窗口无焦点也生效。
 * - `vad`  声控：仅在检测到本机正在说话时传；静音自动停。由本机说话检测喂入。
 *
 * 这里只维护「当前是否允许传输」这个布尔判定，纯状态机、无副作用，便于单测。
 * 真正改 track.enabled 的是 `lobbyCaptureGate`——模式变化时调用它的 refresh 重判。
 */
export type VoiceMode = 'open' | 'ptt' | 'vad';

export const DEFAULT_VOICE_MODE: VoiceMode = 'open';

/** 配置里读出的字符串归一化为合法模式，未知值回落到常开。 */
export function normalizeVoiceMode(value: unknown): VoiceMode {
  return value === 'ptt' || value === 'vad' ? value : 'open';
}

class VoiceModeController {
  private mode: VoiceMode = DEFAULT_VOICE_MODE;
  /** PTT 键当前是否按住。 */
  private pttHeld = false;
  /** VAD：本机当前是否被判定为正在说话。 */
  private speaking = false;
  private listeners = new Set<() => void>();

  getMode(): VoiceMode {
    return this.mode;
  }

  /** 当前模式下是否允许向外发送音频（在「麦克风已开」的前提之上再叠加）。 */
  transmitting(): boolean {
    switch (this.mode) {
      case 'open':
        return true;
      case 'ptt':
        return this.pttHeld;
      case 'vad':
        return this.speaking;
    }
  }

  setMode(mode: VoiceMode): void {
    if (mode === this.mode) return;
    this.mode = mode;
    // 切出 PTT/VAD 时清掉瞬态，避免残留的 held/speaking 影响新模式。
    this.pttHeld = false;
    this.speaking = false;
    this.notify();
  }

  setPttHeld(held: boolean): void {
    if (held === this.pttHeld) return;
    this.pttHeld = held;
    // 仅 PTT 模式下按键变化才影响传输，省去无谓的 refresh。
    if (this.mode === 'ptt') this.notify();
  }

  setSpeaking(speaking: boolean): void {
    if (speaking === this.speaking) return;
    this.speaking = speaking;
    if (this.mode === 'vad') this.notify();
  }

  /** 订阅「允许传输状态可能变化」。返回取消订阅函数。 */
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify(): void {
    for (const listener of this.listeners) {
      try {
        listener();
      } catch (error) {
        console.warn('[voiceMode] 监听器异常（忽略）:', error);
      }
    }
  }
}

export const voiceMode = new VoiceModeController();
