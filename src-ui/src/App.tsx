import { Component, createSignal, onMount, ParentProps } from "solid-js";
import { A, useLocation } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";

// 类型定义
interface DaemonStatus {
  is_running: boolean;
  is_paused: boolean;
  is_idle: boolean;
  last_capture_time: number | null;
  total_captures_today: number;
}

const App: Component<ParentProps> = (props) => {
  const location = useLocation();
  const [status, setStatus] = createSignal<DaemonStatus | null>(null);
  const [loading, setLoading] = createSignal(false);

  // 获取守护进程状态
  const fetchStatus = async () => {
    try {
      const s = await invoke<DaemonStatus>("get_capture_status");
      setStatus(s);
    } catch (e) {
      console.error("Failed to get status:", e);
    }
  };

  // 启动录制
  const startRecording = async () => {
    setLoading(true);
    try {
      await invoke("start_daemon");
      await fetchStatus();
    } catch (e) {
      console.error("Failed to start daemon:", e);
    } finally {
      setLoading(false);
    }
  };

  // 停止录制
  const stopRecording = async () => {
    setLoading(true);
    try {
      await invoke("stop_daemon");
      await fetchStatus();
    } catch (e) {
      console.error("Failed to stop daemon:", e);
    } finally {
      setLoading(false);
    }
  };

  // 暂停/恢复录制
  const togglePause = async () => {
    const currentStatus = status();
    if (!currentStatus?.is_running) return;

    setLoading(true);
    try {
      await invoke("toggle_capture", { paused: !currentStatus.is_paused });
      await fetchStatus();
    } catch (e) {
      console.error("Failed to toggle capture:", e);
    } finally {
      setLoading(false);
    }
  };

  onMount(() => {
    fetchStatus();
    // 定期刷新状态
    const interval = setInterval(fetchStatus, 5000);
    return () => clearInterval(interval);
  });

  // 导航项
  const navItems = [
    { path: "/", label: "时间线", icon: "📅" },
    { path: "/search", label: "搜索", icon: "🔍" },
    { path: "/summaries", label: "摘要", icon: "📝" },
    { path: "/entities", label: "实体", icon: "🏷️" },
    { path: "/settings", label: "设置", icon: "⚙️" },
  ];

  return (
    <div class="flex h-screen bg-background">
      {/* 侧边栏 */}
      <nav class="w-48 bg-background-secondary border-r border-gray-700 flex flex-col">
        {/* Logo */}
        <div class="p-4 border-b border-gray-700">
          <h1 class="text-xl font-bold text-white">Engram</h1>
          <p class="text-xs text-foreground-secondary mt-1">语义记忆增强系统</p>
        </div>

        {/* 导航链接 */}
        <div class="flex-1 py-4">
          {navItems.map((item) => (
            <A
              href={item.path}
              class={`flex items-center px-4 py-3 text-sm transition-colors ${
                location.pathname === item.path
                  ? "bg-accent text-white"
                  : "text-foreground-secondary hover:bg-background-card hover:text-white"
              }`}
            >
              <span class="mr-3">{item.icon}</span>
              {item.label}
            </A>
          ))}
        </div>

        {/* 录制控制 */}
        <div class="p-4 border-t border-gray-700 space-y-2">
          {!status()?.is_running ? (
            <button
              onClick={startRecording}
              disabled={loading()}
              class="w-full px-4 py-2 bg-success hover:bg-success/80 disabled:opacity-50 rounded-lg text-sm transition-colors"
            >
              {loading() ? "启动中..." : "开始录制"}
            </button>
          ) : (
            <>
              <button
                onClick={togglePause}
                disabled={loading()}
                class={`w-full px-4 py-2 ${
                  status()?.is_paused
                    ? "bg-accent hover:bg-accent/80"
                    : "bg-warning hover:bg-warning/80"
                } disabled:opacity-50 rounded-lg text-sm transition-colors`}
              >
                {loading() ? "处理中..." : status()?.is_paused ? "恢复录制" : "暂停录制"}
              </button>
              <button
                onClick={stopRecording}
                disabled={loading()}
                class="w-full px-4 py-2 bg-error hover:bg-error/80 disabled:opacity-50 rounded-lg text-sm transition-colors"
              >
                {loading() ? "停止中..." : "停止录制"}
              </button>
            </>
          )}
        </div>

        {/* 状态栏 */}
        <div class="p-4 border-t border-gray-700">
          <div class="flex items-center text-sm">
            <span
              class={`w-2 h-2 rounded-full mr-2 ${
                status()?.is_paused
                  ? "bg-warning"
                  : status()?.is_running
                  ? "bg-success"
                  : "bg-gray-500"
              }`}
            />
            <span class="text-foreground-secondary">
              {status()?.is_paused
                ? "已暂停"
                : status()?.is_running
                ? "录制中"
                : "未启动"}
            </span>
          </div>
          {status()?.total_captures_today !== undefined && (
            <p class="text-xs text-foreground-secondary mt-1">
              今日截图: {status()?.total_captures_today}
            </p>
          )}
        </div>
      </nav>

      {/* 主内容区 */}
      <main class="flex-1 overflow-hidden">
        {props.children}
      </main>
    </div>
  );
};

export default App;
