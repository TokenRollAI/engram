import { Component, createSignal, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

// 类型定义
interface Settings {
  capture_interval_ms: number;
  idle_threshold_ms: number;
  similarity_threshold: number;
  hot_data_days: number;
  warm_data_days: number;
  summary_interval_min: number;
}

interface StorageStats {
  total_traces: number;
  total_summaries: number;
  database_size_bytes: number;
  screenshots_size_bytes: number;
  oldest_trace_time: number | null;
}

interface VlmConfig {
  endpoint: string;
  model: string;
  api_key: string | null;
  max_tokens: number;
  temperature: number;
}

interface EmbeddingConfig {
  endpoint: string | null;
  model: string;
  api_key: string | null;
}

interface VlmTaskConfig {
  interval_ms: number;
  batch_size: number;
  concurrency: number;
  enabled: boolean;
}

interface AiConfig {
  vlm: VlmConfig;
  embedding: EmbeddingConfig;
  vlm_task: VlmTaskConfig;
}

interface AiStatus {
  vlm_ready: boolean;
  embedder_ready: boolean;
  pending_analysis_count: number;
  pending_embedding_count: number;
}

const Settings: Component = () => {
  const [settings, setSettings] = createSignal<Settings | null>(null);
  const [stats, setStats] = createSignal<StorageStats | null>(null);
  const [aiConfig, setAiConfig] = createSignal<AiConfig | null>(null);
  const [aiStatus, setAiStatus] = createSignal<AiStatus | null>(null);
  const [saving, setSaving] = createSignal(false);
  const [savingAi, setSavingAi] = createSignal(false);
  const [message, setMessage] = createSignal<string | null>(null);
  const [activeTab, setActiveTab] = createSignal<"capture" | "ai">("capture");

  // 加载数据
  onMount(async () => {
    try {
      const [s, st, ai, status] = await Promise.all([
        invoke<Settings>("get_settings"),
        invoke<StorageStats>("get_storage_stats"),
        invoke<AiConfig>("get_ai_config"),
        invoke<AiStatus>("get_ai_status"),
      ]);
      setSettings(s);
      setStats(st);
      setAiConfig(ai);
      setAiStatus(status);
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  });

  // 保存设置
  const saveSettings = async () => {
    const s = settings();
    if (!s) return;

    setSaving(true);
    setMessage(null);

    try {
      await invoke("update_settings", { settings: s });
      setMessage("设置已保存");
      setTimeout(() => setMessage(null), 3000);
    } catch (e) {
      console.error("Failed to save settings:", e);
      setMessage("保存失败: " + e);
    } finally {
      setSaving(false);
    }
  };

  // 保存 AI 配置
  const saveAiConfig = async () => {
    const config = aiConfig();
    if (!config) return;

    setSavingAi(true);
    setMessage(null);

    try {
      await invoke("update_ai_config", { config });
      // 刷新 AI 状态
      const status = await invoke<AiStatus>("get_ai_status");
      setAiStatus(status);
      setMessage("AI 配置已保存并重新初始化");
      setTimeout(() => setMessage(null), 3000);
    } catch (e) {
      console.error("Failed to save AI config:", e);
      setMessage("保存失败: " + e);
    } finally {
      setSavingAi(false);
    }
  };

  // 格式化文件大小
  const formatBytes = (bytes: number) => {
    if (bytes < 1024) return bytes + " B";
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
    if (bytes < 1024 * 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + " MB";
    return (bytes / 1024 / 1024 / 1024).toFixed(2) + " GB";
  };

  // 更新设置值
  const updateSetting = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    const s = settings();
    if (s) {
      setSettings({ ...s, [key]: value });
    }
  };

  // 更新 VLM 配置
  const updateVlmConfig = <K extends keyof VlmConfig>(key: K, value: VlmConfig[K]) => {
    const config = aiConfig();
    if (config) {
      setAiConfig({ ...config, vlm: { ...config.vlm, [key]: value } });
    }
  };

  // 更新 Embedding 配置
  const updateEmbeddingConfig = <K extends keyof EmbeddingConfig>(key: K, value: EmbeddingConfig[K]) => {
    const config = aiConfig();
    if (config) {
      setAiConfig({ ...config, embedding: { ...config.embedding, [key]: value } });
    }
  };

  // 更新 VLM 任务配置
  const updateVlmTaskConfig = <K extends keyof VlmTaskConfig>(key: K, value: VlmTaskConfig[K]) => {
    const config = aiConfig();
    if (config) {
      setAiConfig({ ...config, vlm_task: { ...config.vlm_task, [key]: value } });
    }
  };

  // 预设配置
  const vlmPresets = [
    { name: "Ollama (本地)", endpoint: "http://127.0.0.1:11434/v1", model: "qwen3-vl:4b", needsKey: false },
    { name: "vLLM (本地)", endpoint: "http://127.0.0.1:8000/v1", model: "qwen3-vl-4b", needsKey: false },
    { name: "LM Studio (本地)", endpoint: "http://127.0.0.1:1234/v1", model: "local-model", needsKey: false },
    { name: "OpenAI", endpoint: "https://api.openai.com/v1", model: "gpt-4o", needsKey: true },
    { name: "Together AI", endpoint: "https://api.together.xyz/v1", model: "Qwen/Qwen2-VL-72B-Instruct", needsKey: true },
  ];

  const embeddingPresets = [
    { name: "本地 (MiniLM)", endpoint: "", model: "all-MiniLM-L6-v2", needsKey: false },
    { name: "Ollama", endpoint: "http://127.0.0.1:11434/v1", model: "nomic-embed-text", needsKey: false },
    { name: "OpenAI", endpoint: "https://api.openai.com/v1", model: "text-embedding-3-small", needsKey: true },
  ];

  const applyVlmPreset = (preset: typeof vlmPresets[0]) => {
    const config = aiConfig();
    if (config) {
      setAiConfig({
        ...config,
        vlm: {
          ...config.vlm,
          endpoint: preset.endpoint,
          model: preset.model,
          api_key: preset.needsKey ? config.vlm.api_key : null,
        }
      });
    }
  };

  const applyEmbeddingPreset = (preset: typeof embeddingPresets[0]) => {
    const config = aiConfig();
    if (config) {
      setAiConfig({
        ...config,
        embedding: {
          ...config.embedding,
          endpoint: preset.endpoint || null,
          model: preset.model,
          api_key: preset.needsKey ? config.embedding.api_key : null,
        }
      });
    }
  };

  return (
    <div class="h-full overflow-auto">
      <div class="max-w-2xl mx-auto p-6 space-y-6">
        <h2 class="text-2xl font-bold">设置</h2>

        {/* 消息提示 */}
        <Show when={message()}>
          <div
            class={`p-3 rounded ${
              message()?.includes("失败") ? "bg-error/20 text-error" : "bg-success/20 text-success"
            }`}
          >
            {message()}
          </div>
        </Show>

        {/* Tab 切换 */}
        <div class="flex space-x-2 border-b border-gray-700 pb-2">
          <button
            onClick={() => setActiveTab("capture")}
            class={`px-4 py-2 rounded-t-lg transition-colors ${
              activeTab() === "capture"
                ? "bg-background-card text-white"
                : "text-foreground-secondary hover:text-white"
            }`}
          >
            捕获设置
          </button>
          <button
            onClick={() => setActiveTab("ai")}
            class={`px-4 py-2 rounded-t-lg transition-colors ${
              activeTab() === "ai"
                ? "bg-background-card text-white"
                : "text-foreground-secondary hover:text-white"
            }`}
          >
            AI 模型配置
          </button>
        </div>

        {/* 捕获设置 Tab */}
        <Show when={activeTab() === "capture"}>
          {/* 捕获设置 */}
          <section class="bg-background-card rounded-lg p-6">
            <h3 class="text-lg font-semibold mb-4 flex items-center">
              <span class="mr-2">📸</span>
              捕获设置
            </h3>

            <Show when={settings()}>
              <div class="space-y-4">
                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">
                    截图间隔 (毫秒)
                  </label>
                  <input
                    type="number"
                    value={settings()!.capture_interval_ms}
                    onInput={(e) =>
                      updateSetting("capture_interval_ms", parseInt(e.currentTarget.value) || 2000)
                    }
                    min={500}
                    max={60000}
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                  <p class="text-xs text-foreground-secondary mt-1">
                    建议值: 2000ms (2秒)
                  </p>
                </div>

                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">
                    闲置阈值 (毫秒)
                  </label>
                  <input
                    type="number"
                    value={settings()!.idle_threshold_ms}
                    onInput={(e) =>
                      updateSetting("idle_threshold_ms", parseInt(e.currentTarget.value) || 30000)
                    }
                    min={5000}
                    max={300000}
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                  <p class="text-xs text-foreground-secondary mt-1">
                    无操作超过此时间后暂停截图
                  </p>
                </div>

                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">
                    相似度阈值 (汉明距离)
                  </label>
                  <input
                    type="number"
                    value={settings()!.similarity_threshold}
                    onInput={(e) =>
                      updateSetting("similarity_threshold", parseInt(e.currentTarget.value) || 5)
                    }
                    min={0}
                    max={64}
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                  <p class="text-xs text-foreground-secondary mt-1">
                    越小越严格，相似帧会被跳过
                  </p>
                </div>
              </div>
            </Show>
          </section>

          {/* 存储设置 */}
          <section class="bg-background-card rounded-lg p-6">
            <h3 class="text-lg font-semibold mb-4 flex items-center">
              <span class="mr-2">💾</span>
              存储设置
            </h3>

            <Show when={settings()}>
              <div class="space-y-4">
                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">
                    热数据保留天数
                  </label>
                  <input
                    type="number"
                    value={settings()!.hot_data_days}
                    onInput={(e) =>
                      updateSetting("hot_data_days", parseInt(e.currentTarget.value) || 7)
                    }
                    min={1}
                    max={365}
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                  <p class="text-xs text-foreground-secondary mt-1">
                    保留完整截图的天数
                  </p>
                </div>

                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">
                    温数据保留天数
                  </label>
                  <input
                    type="number"
                    value={settings()!.warm_data_days}
                    onInput={(e) =>
                      updateSetting("warm_data_days", parseInt(e.currentTarget.value) || 30)
                    }
                    min={1}
                    max={365}
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                  <p class="text-xs text-foreground-secondary mt-1">
                    仅保留 OCR 文本的天数
                  </p>
                </div>
              </div>
            </Show>
          </section>

          {/* 存储统计 */}
          <section class="bg-background-card rounded-lg p-6">
            <h3 class="text-lg font-semibold mb-4 flex items-center">
              <span class="mr-2">📊</span>
              存储统计
            </h3>

            <Show when={stats()} fallback={<p class="text-foreground-secondary">加载中...</p>}>
              <div class="grid grid-cols-2 gap-4">
                <div>
                  <p class="text-sm text-foreground-secondary">总截图数</p>
                  <p class="text-2xl font-semibold">{stats()!.total_traces.toLocaleString()}</p>
                </div>
                <div>
                  <p class="text-sm text-foreground-secondary">总摘要数</p>
                  <p class="text-2xl font-semibold">{stats()!.total_summaries.toLocaleString()}</p>
                </div>
                <div>
                  <p class="text-sm text-foreground-secondary">数据库大小</p>
                  <p class="text-2xl font-semibold">{formatBytes(stats()!.database_size_bytes)}</p>
                </div>
                <div>
                  <p class="text-sm text-foreground-secondary">截图占用</p>
                  <p class="text-2xl font-semibold">{formatBytes(stats()!.screenshots_size_bytes)}</p>
                </div>
              </div>
            </Show>
          </section>

          {/* 保存按钮 */}
          <div class="flex justify-end">
            <button
              onClick={saveSettings}
              disabled={saving()}
              class="px-6 py-2 bg-accent hover:bg-accent-hover disabled:opacity-50 rounded-lg transition-colors"
            >
              {saving() ? "保存中..." : "保存设置"}
            </button>
          </div>
        </Show>

        {/* AI 配置 Tab */}
        <Show when={activeTab() === "ai"}>
          {/* AI 状态 */}
          <section class="bg-background-card rounded-lg p-6">
            <h3 class="text-lg font-semibold mb-4 flex items-center">
              <span class="mr-2">🤖</span>
              AI 状态
            </h3>

            <Show when={aiStatus()}>
              <div class="grid grid-cols-2 gap-4">
                <div class="flex items-center">
                  <span
                    class={`w-3 h-3 rounded-full mr-2 ${
                      aiStatus()!.vlm_ready ? "bg-success" : "bg-gray-500"
                    }`}
                  />
                  <span>VLM 引擎: {aiStatus()!.vlm_ready ? "已就绪" : "未连接"}</span>
                </div>
                <div class="flex items-center">
                  <span
                    class={`w-3 h-3 rounded-full mr-2 ${
                      aiStatus()!.embedder_ready ? "bg-success" : "bg-gray-500"
                    }`}
                  />
                  <span>嵌入模型: {aiStatus()!.embedder_ready ? "已就绪" : "未初始化"}</span>
                </div>
              </div>
            </Show>
          </section>

          {/* VLM 配置 */}
          <section class="bg-background-card rounded-lg p-6">
            <h3 class="text-lg font-semibold mb-4 flex items-center">
              <span class="mr-2">👁️</span>
              VLM 视觉理解模型
            </h3>

            <Show when={aiConfig()}>
              <div class="space-y-4">
                {/* 预设选择 */}
                <div>
                  <label class="block text-sm text-foreground-secondary mb-2">快速预设</label>
                  <div class="flex flex-wrap gap-2">
                    {vlmPresets.map((preset) => (
                      <button
                        onClick={() => applyVlmPreset(preset)}
                        class="px-3 py-1 text-sm bg-background hover:bg-gray-700 border border-gray-600 rounded transition-colors"
                      >
                        {preset.name}
                      </button>
                    ))}
                  </div>
                </div>

                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">API 端点</label>
                  <input
                    type="text"
                    value={aiConfig()!.vlm.endpoint}
                    onInput={(e) => updateVlmConfig("endpoint", e.currentTarget.value)}
                    placeholder="http://127.0.0.1:11434/v1"
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                </div>

                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">模型名称</label>
                  <input
                    type="text"
                    value={aiConfig()!.vlm.model}
                    onInput={(e) => updateVlmConfig("model", e.currentTarget.value)}
                    placeholder="qwen3-vl:4b"
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                </div>

                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">
                    API 密钥 (远程服务需要)
                  </label>
                  <input
                    type="password"
                    value={aiConfig()!.vlm.api_key || ""}
                    onInput={(e) => updateVlmConfig("api_key", e.currentTarget.value || null)}
                    placeholder="sk-..."
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                </div>

                <div class="grid grid-cols-2 gap-4">
                  <div>
                    <label class="block text-sm text-foreground-secondary mb-1">最大 Tokens</label>
                    <input
                      type="number"
                      value={aiConfig()!.vlm.max_tokens}
                      onInput={(e) => updateVlmConfig("max_tokens", parseInt(e.currentTarget.value) || 512)}
                      min={64}
                      max={4096}
                      class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                    />
                  </div>
                  <div>
                    <label class="block text-sm text-foreground-secondary mb-1">温度</label>
                    <input
                      type="number"
                      value={aiConfig()!.vlm.temperature}
                      onInput={(e) => updateVlmConfig("temperature", parseFloat(e.currentTarget.value) || 0.3)}
                      min={0}
                      max={2}
                      step={0.1}
                      class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                    />
                  </div>
                </div>
              </div>
            </Show>
          </section>

          {/* Embedding 配置 */}
          <section class="bg-background-card rounded-lg p-6">
            <h3 class="text-lg font-semibold mb-4 flex items-center">
              <span class="mr-2">🔤</span>
              文本嵌入模型
            </h3>

            <Show when={aiConfig()}>
              <div class="space-y-4">
                {/* 预设选择 */}
                <div>
                  <label class="block text-sm text-foreground-secondary mb-2">快速预设</label>
                  <div class="flex flex-wrap gap-2">
                    {embeddingPresets.map((preset) => (
                      <button
                        onClick={() => applyEmbeddingPreset(preset)}
                        class="px-3 py-1 text-sm bg-background hover:bg-gray-700 border border-gray-600 rounded transition-colors"
                      >
                        {preset.name}
                      </button>
                    ))}
                  </div>
                </div>

                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">
                    API 端点 (留空使用本地模型)
                  </label>
                  <input
                    type="text"
                    value={aiConfig()!.embedding.endpoint || ""}
                    onInput={(e) => updateEmbeddingConfig("endpoint", e.currentTarget.value || null)}
                    placeholder="留空使用本地 MiniLM 模型"
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                </div>

                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">模型名称</label>
                  <input
                    type="text"
                    value={aiConfig()!.embedding.model}
                    onInput={(e) => updateEmbeddingConfig("model", e.currentTarget.value)}
                    placeholder="text-embedding-3-small"
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                </div>

                <div>
                  <label class="block text-sm text-foreground-secondary mb-1">
                    API 密钥 (远程服务需要)
                  </label>
                  <input
                    type="password"
                    value={aiConfig()!.embedding.api_key || ""}
                    onInput={(e) => updateEmbeddingConfig("api_key", e.currentTarget.value || null)}
                    placeholder="sk-..."
                    class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                  />
                </div>

                <p class="text-xs text-foreground-secondary">
                  本地模式使用 all-MiniLM-L6-v2 模型 (384维)，无需联网。
                  如配置 API 但连接失败，将自动回退到本地模型。
                </p>
              </div>
            </Show>
          </section>

          {/* VLM 任务配置 */}
          <section class="bg-background-card rounded-lg p-6">
            <h3 class="text-lg font-semibold mb-4 flex items-center">
              <span class="mr-2">⚡</span>
              后台分析任务
            </h3>

            <Show when={aiConfig()}>
              <div class="space-y-4">
                <div class="grid grid-cols-3 gap-4">
                  <div>
                    <label class="block text-sm text-foreground-secondary mb-1">并发数</label>
                    <input
                      type="number"
                      value={aiConfig()!.vlm_task.concurrency}
                      onInput={(e) => updateVlmTaskConfig("concurrency", parseInt(e.currentTarget.value) || 3)}
                      min={1}
                      max={10}
                      class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                    />
                    <p class="text-xs text-foreground-secondary mt-1">同时处理的请求数</p>
                  </div>
                  <div>
                    <label class="block text-sm text-foreground-secondary mb-1">批处理大小</label>
                    <input
                      type="number"
                      value={aiConfig()!.vlm_task.batch_size}
                      onInput={(e) => updateVlmTaskConfig("batch_size", parseInt(e.currentTarget.value) || 10)}
                      min={1}
                      max={50}
                      class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                    />
                    <p class="text-xs text-foreground-secondary mt-1">每批处理的截图数</p>
                  </div>
                  <div>
                    <label class="block text-sm text-foreground-secondary mb-1">检查间隔 (秒)</label>
                    <input
                      type="number"
                      value={Math.round(aiConfig()!.vlm_task.interval_ms / 1000)}
                      onInput={(e) => updateVlmTaskConfig("interval_ms", (parseInt(e.currentTarget.value) || 5) * 1000)}
                      min={1}
                      max={60}
                      class="w-full px-3 py-2 bg-background border border-gray-600 rounded focus:outline-none focus:ring-2 focus:ring-accent"
                    />
                    <p class="text-xs text-foreground-secondary mt-1">检查新截图的频率</p>
                  </div>
                </div>

                <div class="flex items-center justify-between p-3 bg-background rounded">
                  <div>
                    <p class="font-medium">启用后台分析</p>
                    <p class="text-xs text-foreground-secondary">自动分析新截图并生成嵌入向量</p>
                  </div>
                  <button
                    onClick={() => updateVlmTaskConfig("enabled", !aiConfig()!.vlm_task.enabled)}
                    class={`relative w-12 h-6 rounded-full transition-colors ${
                      aiConfig()!.vlm_task.enabled ? "bg-accent" : "bg-gray-600"
                    }`}
                  >
                    <span
                      class={`absolute top-1 w-4 h-4 bg-white rounded-full transition-transform ${
                        aiConfig()!.vlm_task.enabled ? "translate-x-7" : "translate-x-1"
                      }`}
                    />
                  </button>
                </div>

                <p class="text-xs text-foreground-secondary">
                  提示：并发数越高处理速度越快，但会增加 API 调用压力。
                  建议本地模型设为 1-2，云端 API 设为 3-5。
                </p>
              </div>
            </Show>
          </section>

          {/* 保存按钮 */}
          <div class="flex justify-end">
            <button
              onClick={saveAiConfig}
              disabled={savingAi()}
              class="px-6 py-2 bg-accent hover:bg-accent-hover disabled:opacity-50 rounded-lg transition-colors"
            >
              {savingAi() ? "保存中..." : "保存 AI 配置"}
            </button>
          </div>
        </Show>

        {/* 关于 */}
        <section class="text-center text-sm text-foreground-secondary pt-8 border-t border-gray-700">
          <p>Engram v0.1.0</p>
          <p class="mt-1">本地优先的语义记忆增强系统</p>
        </section>
      </div>
    </div>
  );
};

export default Settings;
