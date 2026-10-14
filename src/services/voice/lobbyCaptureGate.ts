import { voiceMode } from './voiceMode';

/** A recording lease silences all current and newly attached lobby tracks. */
export class LobbyCaptureGate {
  private leases = new Set<symbol>();
  private tracks = new Map<MediaStreamTrack, () => boolean>();
  constructor() {
    // 语音模式（常开 / 按键说话 / 声控）变化时重新判定所有轨道的传输开关。
    voiceMode.subscribe(() => this.apply());
  }
  register(track: MediaStreamTrack, canTransmit: () => boolean) {
    this.tracks.set(track, canTransmit);
    track.addEventListener('ended', () => this.tracks.delete(track), { once: true });
    this.apply();
  }
  private apply() {
    // 语音模式门控：PTT 未按住 / VAD 未检测到说话时，一律静音——与录制租约叠加。
    const modeAllows = voiceMode.transmitting();
    for (const [track, canTransmit] of this.tracks) {
      if (track.readyState === 'ended') { this.tracks.delete(track); continue; }
      track.enabled = this.leases.size === 0 && modeAllows && canTransmit();
    }
  }
  suspend(): () => void {
    const lease = Symbol();
    this.leases.add(lease);
    this.apply();
    return () => { this.leases.delete(lease); this.apply(); };
  }
}
export const lobbyCaptureGate = new LobbyCaptureGate();
