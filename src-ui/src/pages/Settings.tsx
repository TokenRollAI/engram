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

const Settings: Component = () => {
  const [settings, setSettings] = createSignal<Settings | null>(null);
  const [stats, setStats] = createSignal<StorageStats | null>(null);
  const [saving, setSaving] = createSignal(false);
  const [message, setMessage] = createSignal<string | null>(null);

  // 加载数据
  onMount(async () => {
    try {
      const [s, st] = await Promise.all([
        invoke<Settings>("get_settings"),
        invoke<StorageStats>("get_storage_stats"),
      ]);
      setSettings(s);
      setStats(st);
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

  return (
    <div class="h-full overflow-auto">
      <div class="max-w-2xl mx-auto p-6 space-y-8">
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
