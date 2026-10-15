/**
 * 语音设备设置
 * - 选择麦克风(输入)与扬声器(输出)设备
 * - 麦克风试音：实时电平条
 * - 扬声器试音：在选定输出设备上播放测试音
 * 说明：输入设备会在下次开启/重开麦克风时生效；输出设备对已连接对端实时生效。
 *
 * 该模块导出两部分：
 * - VoiceDevicePanel：可内嵌的设置面板（用于「大厅动态设置」中集成）
 * - VoiceSettings：独立弹窗（兼容旧调用，内部复用 VoiceDevicePanel）
 */

import React, { useState, useEffect, useRef } from 'react';
import { Modal, Select, Button, Typography, Space, Progress, Switch, Slider, message } from 'antd';
import { useTranslation } from 'react-i18next';
import { tl } from '../../i18n';
import { audioDevices } from '../../services/voice/audioDevices';
import { microphoneDevices, nativeMicrophoneLevel, openMicrophone, resumeAudioContext } from '../../services/voice/nativeMicrophone';
import { microphoneLevelPercent, pcmRms } from '../../services/voice/microphoneLevel';
import { macHeadphoneMode, setMacHeadphoneMode, macOutputIsSpeaker, macMicGain, setMacMicGain, MAC_MIC_GAIN_MIN, MAC_MIC_GAIN_MAX } from '../../services/voice/macEchoMode';
import { isMacOSPlatform } from '../../utils/platform';
import { webrtcClient } from '../../services';

const { Text } = Typography;

interface DeviceOption {
  value: string;
  label: string;
}

interface VoiceDevicePanelProps {
  /** 面板是否处于激活状态（如所在弹窗是否打开），用于控制设备枚举与试音清理 */
  active?: boolean;
}

/**
 * 语音设备设置面板（无弹窗外壳，可内嵌）
 */
export const VoiceDevicePanel: React.FC<VoiceDevicePanelProps> = ({ active = true }) => {
  useTranslation();
  const [inputs, setInputs] = useState<DeviceOption[]>([]);
  const [outputs, setOutputs] = useState<DeviceOption[]>([]);
  const [inputId, setInputId] = useState<string>('');
  const [outputId, setOutputId] = useState<string>('');
  const [supportsOutput, setSupportsOutput] = useState(true);
  // macOS 专属：耳机低延迟模式（关掉平台音频处理，断开 VPIO 全双工耦合）
  const [headphoneMode, setHeadphoneMode] = useState(macHeadphoneMode());
  // 低延迟模式关掉了平台 AGC，音量由这个软件增益决定。
  const [micGain, setMicGain] = useState(macMicGain());

  // 麦克风试音电平
  const [testing, setTesting] = useState(false);
  const [level, setLevel] = useState(0);
  const testStreamRef = useRef<MediaStream | null>(null);
  const ctxRef = useRef<AudioContext | null>(null);
  const rafRef = useRef<number | null>(null);
  const testGeneration = useRef(0);

  // 下拉框渲染到自身父节点，避免在透明窗口/弹窗中出现层级错误（显示在弹窗背后无法点击）
  const popupContainer = (triggerNode: HTMLElement) =>
    (triggerNode.parentElement as HTMLElement) || document.body;

  const loadDevices = async () => {
    try {
      // Enumerating inputs does not open the microphone or start recording.
      const [microphones, browserDevices] = await Promise.all([
        microphoneDevices(), navigator.mediaDevices.enumerateDevices(),
      ]);
      const devices = [...microphones, ...browserDevices.filter(device => device.kind === 'audiooutput')];
      const defaultInput = microphones.find(device => device.isDefault);
      const ins: DeviceOption[] = [{ value: '', label: `${tl('系统默认麦克风', 'System Default Microphone')}${defaultInput ? `（${defaultInput.label}）` : ''}` }];
      const communicationsInput = microphones.find(device => device.isCommunications);
      if (communicationsInput && communicationsInput.deviceId !== defaultInput?.deviceId) {
        ins.push({ value: 'communications', label: `${tl('默认通信麦克风', 'Default Communications Microphone')}（${communicationsInput.label}）` });
      }
      const outs: DeviceOption[] = [{ value: '', label: tl('系统默认扬声器', 'System Default Speaker') }];
      devices.forEach((d) => {
        if (d.kind === 'audioinput') {
          ins.push({ value: d.deviceId, label: d.label || `${tl('麦克风', 'Microphone')} ${ins.length}` });
        } else if (d.kind === 'audiooutput') {
          outs.push({ value: d.deviceId, label: d.label || `${tl('扬声器', 'Speaker')} ${outs.length}` });
        }
      });
      setInputs(ins);
      setOutputs(outs);
      const savedInputId = audioDevices.getInputDeviceId();
      if (savedInputId && !ins.some(option => option.value === savedInputId)) {
        audioDevices.setInputDeviceId('');
        setInputId('');
      }
      const savedOutputId = audioDevices.getOutputDeviceId();
      if (savedOutputId) {
        const savedOutput = outs.find((option) => option.value === savedOutputId);
        if (savedOutput) audioDevices.setOutputDeviceId(savedOutputId, savedOutput.label);
      }
      setSupportsOutput(typeof (HTMLMediaElement.prototype as any).setSinkId === 'function');
    } catch (e) {
      message.error(`${tl('枚举音频设备失败', 'Failed to enumerate audio devices')}：${e}`);
    }
  };

  const stopMicTest = () => {
    testGeneration.current++;
    if (rafRef.current !== null) {
      window.cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    if (testStreamRef.current) {
      testStreamRef.current.getTracks().forEach((t) => t.stop());
      testStreamRef.current = null;
    }
    if (ctxRef.current) {
      ctxRef.current.close().catch(() => {});
      ctxRef.current = null;
    }
    setTesting(false);
    setLevel(0);
  };

  useEffect(() => {
    if (active) {
      setInputId(audioDevices.getInputDeviceId());
      setOutputId(audioDevices.getOutputDeviceId());
      void loadDevices();
    } else {
      stopMicTest();
    }
    return () => stopMicTest();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  const handleInputChange = (id: string) => {
    setInputId(id);
    audioDevices.setInputDeviceId(id);
    // 若正在试音，使用新设备重启
    if (testing) {
      stopMicTest();
      void startMicTest(id);
    }
    message.success(tl('麦克风已切换，将在下次开启麦克风时生效', 'Microphone switched, effective next time you enable it'));
  };

  const handleOutputChange = async (id: string) => {
    const previous = audioDevices.getOutputDeviceId();
    try {
      // Validate even before any peer has joined the call.
      const probe = new Audio();
      if (typeof probe.setSinkId === 'function') await probe.setSinkId(id);
      await webrtcClient.applyOutputDeviceToAll(id);
      setOutputId(id);
      audioDevices.setOutputDeviceId(id, outputs.find((option) => option.value === id)?.label || '');
      // 切到外放扬声器时必须关掉耳机低延迟模式（外放无 AEC 会回声），避免用户忘记后误判。
      // 在写入新设备 id 之后再判断，macOutputIsSpeaker 才能读到正确的当前输出。
      if (isMacOSPlatform && headphoneMode && (await macOutputIsSpeaker())) {
        setHeadphoneMode(false);
        setMacHeadphoneMode(false);
        void webrtcClient.refreshMicrophoneProcessing().catch(() => undefined);
        message.warning(
          tl(
            '已切换到扬声器，耳机低延迟模式已自动关闭（外放需要回声消除）。',
            'Switched to speakers; headphone low-latency mode was turned off automatically (speakers need echo cancellation).',
          ),
        );
        return;
      }
      message.success(tl('扬声器已切换并对当前通话生效', 'Speaker switched and applied to the current call'));
    } catch (error) {
      await webrtcClient.applyOutputDeviceToAll(previous).catch(() => {});
      message.error(`${tl('扬声器切换失败，请重新选择可用设备', 'Could not switch speaker; select an available device')}：${error}`);
    }
  };

  const startMicTest = async (deviceId?: string) => {
    stopMicTest();
    const generation = ++testGeneration.current;
    try {
      const id = deviceId !== undefined ? deviceId : inputId;
      const stream = await openMicrophone(id);
      if (generation !== testGeneration.current) { stream.getTracks().forEach(track => track.stop()); return; }
      stream.getAudioTracks()[0]?.addEventListener('ended', () => {
        if (generation === testGeneration.current) stopMicTest();
      }, { once: true });
      testStreamRef.current = stream;
      let readRms: () => number;
      if (nativeMicrophoneLevel(stream) !== undefined) {
        // Meter native PCM directly: no second audio engine, resampling or byte quantization.
        readRms = () => nativeMicrophoneLevel(stream) ?? 0;
      } else {
        const ctx = new (window.AudioContext || (window as any).webkitAudioContext)();
        ctxRef.current = ctx;
        await resumeAudioContext(ctx);
        if (generation !== testGeneration.current) { stream.getTracks().forEach(track => track.stop()); void ctx.close().catch(() => {}); return; }
        const source = ctx.createMediaStreamSource(stream);
        const analyser = ctx.createAnalyser();
        analyser.fftSize = 1024;
        const silent = ctx.createGain();
        silent.gain.value = 0;
        source.connect(analyser);
        analyser.connect(silent).connect(ctx.destination);
        const data = new Float32Array(analyser.fftSize);
        readRms = () => { analyser.getFloatTimeDomainData(data); return pcmRms(data); };
      }
      setTesting(true);
      const tick = () => {
        if (generation !== testGeneration.current) return;
        setLevel(microphoneLevelPercent(readRms()));
        rafRef.current = window.requestAnimationFrame(tick);
      };
      rafRef.current = window.requestAnimationFrame(tick);
    } catch (e) {
      if (generation !== testGeneration.current) return;
      stopMicTest();
      // 系统未授权麦克风时弹出授权引导（与开麦同一套流程），而不是只报一句错。
      if (e instanceof DOMException && (e.name === 'NotAllowedError' || e.name === 'PermissionDeniedError')) {
        window.dispatchEvent(new CustomEvent('mctier-microphone-permission-required'));
        return;
      }
      message.error(`${tl('无法打开麦克风试音', 'Unable to start microphone test')}：${e}`);
    }
  };

  // 扬声器试音：播放一段测试音并路由到选定输出设备
  const testOutput = async () => {
    const ctx = new AudioContext();
    const audio = new Audio();
    let stream: MediaStream | undefined;
    try {
      await resumeAudioContext(ctx);
      const dest = ctx.createMediaStreamDestination();
      stream = dest.stream;
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      gain.gain.value = 0.15;
      osc.frequency.value = 660;
      osc.connect(gain);
      gain.connect(dest);
      osc.start();

      audio.srcObject = dest.stream;
      if (outputId && typeof (audio as any).setSinkId === 'function') {
        await (audio as any).setSinkId(outputId);
      }
      await audio.play();
      await new Promise(resolve => setTimeout(resolve, 600));
    } catch (e) {
      message.error(`${tl('扬声器试音失败', 'Speaker test failed')}：${e}`);
    } finally {
      audio.pause();
      audio.srcObject = null;
      stream?.getTracks().forEach(track => track.stop());
      await ctx.close().catch(() => {});
    }
  };

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <div>
        <Text strong>{tl('麦克风（输入）', 'Microphone (Input)')}</Text>
        <Select
          style={{ width: '100%', marginTop: 6 }}
          value={inputId}
          onChange={handleInputChange}
          options={inputs}
          getPopupContainer={popupContainer}
        />
        <div style={{ marginTop: 10 }}>
          <Space>
            {!testing ? (
              <Button size="small" onClick={() => void startMicTest()}>{tl('开始试音', 'Start Test')}</Button>
            ) : (
              <Button size="small" danger onClick={stopMicTest}>{tl('停止试音', 'Stop Test')}</Button>
            )}
            <Text type="secondary" style={{ fontSize: 12 }}>{tl('说话并观察下方电平', 'Speak and watch the level below')}</Text>
          </Space>
          <Progress percent={level} showInfo={false} strokeColor={level > 60 ? '#52c41a' : '#1677ff'} style={{ marginTop: 6 }} />
        </div>
      </div>

      <div>
        <Text strong>{tl('扬声器（输出）', 'Speaker (Output)')}</Text>
        {supportsOutput ? (
          <>
            <Select
              style={{ width: '100%', marginTop: 6 }}
              value={outputId}
              onChange={handleOutputChange}
              options={outputs}
              getPopupContainer={popupContainer}
            />
            <div style={{ marginTop: 10 }}>
              <Button size="small" onClick={() => void testOutput()}>{tl('扬声器试音', 'Test Speaker')}</Button>
            </div>
          </>
        ) : (
          <div style={{ marginTop: 6 }}>
            <Text type="secondary">{tl('当前环境不支持切换输出设备，将使用系统默认扬声器。', 'Switching output device is not supported here; the system default speaker will be used.')}</Text>
          </div>
        )}
      </div>

      {isMacOSPlatform && (
        <div>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }}>
            <Text strong>{tl('耳机低延迟模式', 'Headphone Low-Latency Mode')}</Text>
            <Switch
              checked={headphoneMode}
              onChange={value => {
                void (async () => {
                  // 防呆：外放扬声器必须保留 AEC，否则对方会听到回声。开启前先确认输出不是扬声器，
                  // 是则拒绝开启并说明原因，避免「忘了关 + 外放」这个最糟组合。
                  if (value && (await macOutputIsSpeaker())) {
                    setHeadphoneMode(false);
                    setMacHeadphoneMode(false);
                    message.warning(
                      tl(
                        '检测到当前使用外放扬声器，无法开启低延迟模式：外放必须保留回声消除，否则对方会听到回声。请先切换到耳机。',
                        'Speakers detected; low-latency mode cannot be enabled. Speakers require echo cancellation or others will hear an echo. Switch to headphones first.',
                      ),
                    );
                    return;
                  }
                  setHeadphoneMode(value);
                  setMacHeadphoneMode(value);
                  // 若当前正在试音，同步以新模式重启试音流，使试音电平条立即反映切换后的信号。
                  if (testing) {
                    stopMicTest();
                    void startMicTest(inputId);
                  }
                  await webrtcClient.refreshMicrophoneProcessing().catch(() =>
                    message.error(tl('切换失败，请重新开启麦克风使其生效', 'Switch failed; re-enable the microphone to apply')),
                  );
                })();
              }}
            />
          </div>
          <div style={{ marginTop: 4 }}>
            <Text type="secondary" style={{ fontSize: 12 }}>
              {headphoneMode
                ? tl(
                    '已开启：当前使用耳机时更流畅，但对方外放时会听到回声。重启软件会自动恢复关闭；若改用外放请手动关闭。',
                    'On: smoother while you wear headphones, but others on speakers will hear an echo. Restarting restores the default; turn this off if you switch to speakers.',
                  )
                : tl(
                    '戴耳机时如果语音卡顿才打开：关闭回声消除换取流畅的双向通话。使用外放扬声器请保持关闭，否则对方会听到回声。重启软件后本开关自动回到关闭。',
                    'Turn on only if voice stutters while wearing headphones: disables echo cancellation for smoother two-way audio. Keep off when using speakers, otherwise others hear an echo. This resets to off after a restart.',
                  )}
            </Text>
          </div>
          {headphoneMode && (
            <div style={{ marginTop: 10 }}>
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }}>
                <Text>{tl('麦克风音量', 'Microphone Level')}</Text>
                <Text type="secondary" style={{ fontSize: 12 }}>{`${micGain.toFixed(1)}×`}</Text>
              </div>
              {/* 低延迟模式关掉了平台的自动增益，音量改由这里的软件增益决定。
                  setMacMicGain 会通过订阅直接实时修改活动 GainNode.gain.value，
                  无需重建麦克风流，通话与试音电平条均即时生效。 */}
              <Slider
                min={MAC_MIC_GAIN_MIN}
                max={MAC_MIC_GAIN_MAX}
                step={0.5}
                value={micGain}
                onChange={value => {
                  setMicGain(value);
                  setMacMicGain(value);
                }}
              />
              <Text type="secondary" style={{ fontSize: 12 }}>
                {tl(
                  '对方听不清就调大，出现破音就调小。',
                  'Raise if others can barely hear you; lower if the audio distorts.',
                )}
              </Text>
            </div>
          )}
        </div>
      )}
    </Space>
  );
};

interface VoiceSettingsProps {
  visible: boolean;
  onClose: () => void;
}

/**
 * 语音设备设置独立弹窗（兼容旧调用）
 */
export const VoiceSettings: React.FC<VoiceSettingsProps> = ({ visible, onClose }) => {
  useTranslation();
  return (
    <Modal title={tl('语音设备设置', 'Voice Device Settings')} open={visible} onCancel={onClose} footer={[
      <Button key="close" type="primary" onClick={onClose}>{tl('完成', 'Done')}</Button>,
    ]} width={460} centered>
      <VoiceDevicePanel active={visible} />
    </Modal>
  );
};
