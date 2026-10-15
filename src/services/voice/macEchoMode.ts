/**
 * macOS 耳机低延迟模式开关（仅 macOS）。
 *
 * macOS 走 WKWebView 的 getUserMedia 采集。打开 echoCancellation（AEC 回声消除）时，
 * WebKit 会把采集与播放交给 Apple 的 VPIO 全双工语音处理单元：它需要播放信号作为
 * 回声参考，于是把「采集」和「播放」耦合进同一个处理周期。双向同时收发时这个单元
 * 成为瓶颈，语音会卡顿（单向正常）。戴耳机没有「扬声器→麦克风」的回声通路，
 * AEC 的参考回路纯属多余，关掉它即可解除这个耦合。
 *
 * 因此提供这个开关，但它是**有代价**的，必须防呆：
 * - 默认关（保留 AEC/AGC）：适合外放扬声器，不会让对方听到回声。
 * - 打开（仅关 AEC，保留 AGC）：戴耳机且遇到卡顿时使用，换取流畅双向通话。
 *
 * 三个防呆措施，避免用户忘记关闭后误以为软件坏了：
 * 1. **不跨启动保留**：只在本次 app 运行期有效，重启回到安全默认。用户「忘记关」
 *    最多影响当前这一次；下次启动自动恢复。
 * 2. **检测到扬声器自动回退**：开启期间若输出是外放扬声器，会自动关掉开关（外放必须
 *    有 AEC 否则会回声），从根上杜绝「忘了关 + 外放」这个最糟组合。
 * 3. **状态可见**：调用方可在界面上显示当前处于低延迟模式，并一键恢复正常。
 *
 * 其它平台不使用本开关（Windows 走原生 WASAPI 采集，Linux 行为不变）。
 */

const KEY = 'mctier_macos_headphone_mode';

/** 本次 app 运行期内的开关状态（不读 localStorage，保证不跨启动残留）。 */
let sessionEnabled = false;

/** 耳机低延迟模式是否开启。 */
export function macHeadphoneMode(): boolean {
  return sessionEnabled;
}

export function setMacHeadphoneMode(on: boolean): void {
  sessionEnabled = on;
  // 写 localStorage 仅用于「开关是否被用户主动打开过」的调试可见性，读取一律以内存为准，
  // 因此重启后必然回到关闭状态——这正是防呆点。写入失败不影响功能。
  try {
    if (on) localStorage.setItem(KEY, 'on');
    else localStorage.removeItem(KEY);
  } catch {
    /* ignore */
  }
}

/**
 * app 启动时调用：清掉可能残留在存储里的旧值，确保每次启动都从安全默认（关闭）开始。
 * 旧版本曾把开关持久化，升级上来的用户会残留 'on'，必须主动清除。
 */
export function resetMacHeadphoneModeOnStartup(): void {
  sessionEnabled = false;
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* ignore */
  }
}

const GAIN_KEY = 'mctier_macos_mic_gain';

/**
 * 耳机低延迟模式下的软件麦克风增益。
 *
 * 关掉 echoCancellation 会同时失去 macOS 只有 VPIO 才提供的 AGC，原始麦克风信号会明显
 * 偏小（用户实测：电平条有跳动但对方听不清）。这里用一个我们完全掌控的软件增益级补回
 * 音量，并对增益值提供调节，避免「一刀切」既不够响又可能削顶。
 *
 * 返回 1.0 表示不额外增益（关闭软件增益级）。范围与默认值集中在
 * {@link MAC_MIC_GAIN_MIN} / {@link MAC_MIC_GAIN_MAX} / {@link MAC_MIC_GAIN_DEFAULT}。
 */
export const MAC_MIC_GAIN_MIN = 1;
export const MAC_MIC_GAIN_MAX = 8;
export const MAC_MIC_GAIN_DEFAULT = 3;

export function macMicGain(): number {
  try {
    const raw = localStorage.getItem(GAIN_KEY);
    if (raw === null) return MAC_MIC_GAIN_DEFAULT;
    const value = Number(raw);
    if (!Number.isFinite(value)) return MAC_MIC_GAIN_DEFAULT;
    return Math.min(MAC_MIC_GAIN_MAX, Math.max(MAC_MIC_GAIN_MIN, value));
  } catch {
    return MAC_MIC_GAIN_DEFAULT;
  }
}

const GAIN_LISTENERS = new Set<(gain: number) => void>();

export function setMacMicGain(value: number): void {
  const clamped = Math.min(MAC_MIC_GAIN_MAX, Math.max(MAC_MIC_GAIN_MIN, value));
  try {
    localStorage.setItem(GAIN_KEY, String(clamped));
  } catch {
    /* ignore */
  }
  GAIN_LISTENERS.forEach(fn => {
    try { fn(clamped); } catch { /* ignore */ }
  });
}

/** 订阅麦克风软件增益变化（供活动的 GainNode 实时平滑更新，无需重建流）。 */
export function subscribeMacMicGain(listener: (gain: number) => void): () => void {
  GAIN_LISTENERS.add(listener);
  return () => GAIN_LISTENERS.delete(listener);
}

/**
 * 当前 macOS 输出设备是否是外放扬声器（而非耳机）。
 *
 * 只有在「确认是扬声器」时才返回 true；设备名认不出时返回 false（按耳机处理），
 * 因为误判成扬声器会让耳机用户白白失去低延迟能力，而误判成耳机最多是外放有回声、
 * 且由调用方在检测到扬声器时自动回退开关兜底。
 */
export async function macOutputIsSpeaker(): Promise<boolean> {
  try {
    const outputs = (await navigator.mediaDevices.enumerateDevices()).filter(
      device => device.kind === 'audiooutput',
    );
    let selectedId = '';
    try {
      selectedId = localStorage.getItem('mctier_audio_output_device') || '';
    } catch {
      /* ignore */
    }
    let match = selectedId ? outputs.find(device => device.deviceId === selectedId) : undefined;
    if (!match) {
      // 跟随系统默认：'default' 别名的 groupId 指向真实设备，取其真名更可靠。
      const fallback = outputs.find(device => device.deviceId === 'default');
      match =
        (fallback?.groupId &&
          outputs.find(
            device => device.deviceId !== 'default' && device.groupId === fallback.groupId,
          )) ||
        fallback ||
        outputs[0];
    }
    const label = (match?.label || '').toLowerCase();
    if (!label) return false; // 认不出 → 按耳机处理
    // 外放：内建/外接扬声器。耳机(headphone)/入耳(earbud/airpods)/头戴(headset) 不匹配。
    return /speaker|扬声器|外放|built-in output/.test(label);
  } catch {
    return false;
  }
}
