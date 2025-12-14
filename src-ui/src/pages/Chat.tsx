import { Component, createSignal, For, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

// 类型定义
interface ChatRequest {
  message: string;
  start_time: number | null;
  end_time: number | null;
  app_filter: string[] | null;
}

interface ChatResponse {
  content: string;
  context_count: number;
  time_range: string | null;
}

interface Message {
  role: "user" | "assistant";
  content: string;
  context_count?: number;
  time_range?: string;
}

const Chat: Component = () => {
  const [messages, setMessages] = createSignal<Message[]>([]);
  const [input, setInput] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [availableApps, setAvailableApps] = createSignal<string[]>([]);
  const [selectedApps, setSelectedApps] = createSignal<string[]>([]);
  const [timeRange, setTimeRange] = createSignal<"today" | "week" | "month" | "all">("today");
  const [showFilters, setShowFilters] = createSignal(false);

  // 获取时间戳（毫秒级，与后端数据库保持一致）
  const getTimeRange = (): { start: number; end: number } => {
    const now = Date.now();
    const day = 24 * 3600 * 1000; // 毫秒

    switch (timeRange()) {
      case "today":
        return { start: now - day, end: now };
      case "week":
        return { start: now - 7 * day, end: now };
      case "month":
        return { start: now - 30 * day, end: now };
      case "all":
        return { start: 0, end: now };
      default:
        return { start: now - day, end: now };
    }
  };

  // 加载可用应用列表
  const loadApps = async () => {
    try {
      const { start, end } = getTimeRange();
      const apps = await invoke<string[]>("get_available_apps", {
        startTime: start,
        endTime: end,
      });
      setAvailableApps(apps);
    } catch (e) {
      console.error("Failed to load apps:", e);
    }
  };

  onMount(loadApps);

  // 发送消息
  const sendMessage = async () => {
    const msg = input().trim();
    if (!msg || loading()) return;

    // 添加用户消息
    setMessages((prev) => [...prev, { role: "user", content: msg }]);
    setInput("");
    setLoading(true);

    try {
      const { start, end } = getTimeRange();
      const request: ChatRequest = {
        message: msg,
        start_time: start,
        end_time: end,
        app_filter: selectedApps().length > 0 ? selectedApps() : null,
      };

      const response = await invoke<ChatResponse>("chat_with_memory", { request });

      // 添加助手回复
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          content: response.content,
          context_count: response.context_count,
          time_range: response.time_range || undefined,
        },
      ]);
    } catch (e) {
      // 添加错误消息
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          content: `抱歉，发生了错误: ${e}`,
        },
      ]);
    } finally {
      setLoading(false);
    }
  };

  // 切换应用选择
  const toggleApp = (app: string) => {
    setSelectedApps((prev) =>
      prev.includes(app) ? prev.filter((a) => a !== app) : [...prev, app]
    );
  };

  // 清空对话
  const clearChat = () => {
    setMessages([]);
  };

  // 时间范围变更时重新加载应用
  const handleTimeRangeChange = (range: "today" | "week" | "month" | "all") => {
    setTimeRange(range);
    setSelectedApps([]);
    loadApps();
  };

  return (
    <div class="h-full flex flex-col bg-background">
      {/* 头部 */}
      <div class="p-4 border-b border-gray-700">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-xl font-bold">记忆对话</h2>
            <p class="text-sm text-foreground-secondary">
              基于屏幕记录与 AI 进行对话
            </p>
          </div>
          <div class="flex items-center gap-2">
            <button
              onClick={() => setShowFilters(!showFilters())}
              class={`px-3 py-1.5 text-sm rounded transition-colors ${
                showFilters() ? "bg-accent text-white" : "bg-background-card hover:bg-gray-700"
              }`}
            >
              筛选
            </button>
            <button
              onClick={clearChat}
              class="px-3 py-1.5 text-sm bg-background-card hover:bg-gray-700 rounded transition-colors"
            >
              清空
            </button>
          </div>
        </div>

        {/* 筛选面板 */}
        <Show when={showFilters()}>
          <div class="mt-4 p-4 bg-background-card rounded-lg space-y-4">
            {/* 时间范围 */}
            <div>
              <label class="block text-sm text-foreground-secondary mb-2">时间范围</label>
              <div class="flex gap-2">
                {(["today", "week", "month", "all"] as const).map((range) => (
                  <button
                    onClick={() => handleTimeRangeChange(range)}
                    class={`px-3 py-1.5 text-sm rounded transition-colors ${
                      timeRange() === range
                        ? "bg-accent text-white"
                        : "bg-background hover:bg-gray-700"
                    }`}
                  >
                    {range === "today"
                      ? "今天"
                      : range === "week"
                      ? "本周"
                      : range === "month"
                      ? "本月"
                      : "全部"}
                  </button>
                ))}
              </div>
            </div>

            {/* 应用过滤 */}
            <div>
              <label class="block text-sm text-foreground-secondary mb-2">
                应用过滤 {selectedApps().length > 0 && `(已选 ${selectedApps().length})`}
              </label>
              <div class="flex flex-wrap gap-2 max-h-24 overflow-y-auto">
                <For each={availableApps()}>
                  {(app) => (
                    <button
                      onClick={() => toggleApp(app)}
                      class={`px-2 py-1 text-xs rounded transition-colors ${
                        selectedApps().includes(app)
                          ? "bg-accent text-white"
                          : "bg-background hover:bg-gray-700"
                      }`}
                    >
                      {app}
                    </button>
                  )}
                </For>
                <Show when={availableApps().length === 0}>
                  <span class="text-sm text-foreground-secondary">
                    该时间范围内没有记录的应用
                  </span>
                </Show>
              </div>
            </div>
          </div>
        </Show>
      </div>

      {/* 消息列表 */}
      <div class="flex-1 overflow-y-auto p-4 space-y-4">
        <Show
          when={messages().length > 0}
          fallback={
            <div class="h-full flex items-center justify-center">
              <div class="text-center text-foreground-secondary">
                <p class="text-4xl mb-4">💬</p>
                <p class="text-lg">开始与你的记忆对话</p>
                <p class="text-sm mt-2">
                  你可以询问关于屏幕活动的问题，例如：
                </p>
                <div class="mt-4 space-y-2">
                  <button
                    onClick={() => setInput("今天我都做了什么？")}
                    class="block w-full px-4 py-2 text-sm bg-background-card hover:bg-gray-700 rounded transition-colors"
                  >
                    今天我都做了什么？
                  </button>
                  <button
                    onClick={() => setInput("我最近在研究什么项目？")}
                    class="block w-full px-4 py-2 text-sm bg-background-card hover:bg-gray-700 rounded transition-colors"
                  >
                    我最近在研究什么项目？
                  </button>
                  <button
                    onClick={() => setInput("帮我回忆一下之前看的那篇文章")}
                    class="block w-full px-4 py-2 text-sm bg-background-card hover:bg-gray-700 rounded transition-colors"
                  >
                    帮我回忆一下之前看的那篇文章
                  </button>
                </div>
              </div>
            </div>
          }
        >
          <For each={messages()}>
            {(message) => (
              <div
                class={`flex ${message.role === "user" ? "justify-end" : "justify-start"}`}
              >
                <div
                  class={`max-w-[80%] p-4 rounded-lg ${
                    message.role === "user"
                      ? "bg-accent text-white"
                      : "bg-background-card"
                  }`}
                >
                  <p class="whitespace-pre-wrap">{message.content}</p>
                  <Show when={message.role === "assistant" && message.context_count !== undefined}>
                    <p class="text-xs text-foreground-secondary mt-2">
                      基于 {message.context_count} 条记录
                      {message.time_range && ` | ${message.time_range}`}
                    </p>
                  </Show>
                </div>
              </div>
            )}
          </For>

          {/* 加载指示器 */}
          <Show when={loading()}>
            <div class="flex justify-start">
              <div class="bg-background-card p-4 rounded-lg">
                <div class="flex items-center space-x-2">
                  <div class="w-2 h-2 bg-accent rounded-full animate-bounce" />
                  <div class="w-2 h-2 bg-accent rounded-full animate-bounce [animation-delay:0.1s]" />
                  <div class="w-2 h-2 bg-accent rounded-full animate-bounce [animation-delay:0.2s]" />
                </div>
              </div>
            </div>
          </Show>
        </Show>
      </div>

      {/* 输入区域 */}
      <div class="p-4 border-t border-gray-700">
        <div class="flex gap-2">
          <input
            type="text"
            value={input()}
            onInput={(e) => setInput(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && sendMessage()}
            placeholder="输入你的问题..."
            disabled={loading()}
            class="flex-1 px-4 py-3 bg-background-card border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-50"
          />
          <button
            onClick={sendMessage}
            disabled={loading() || !input().trim()}
            class="px-6 py-3 bg-accent hover:bg-accent-hover disabled:opacity-50 rounded-lg transition-colors"
          >
            {loading() ? "发送中..." : "发送"}
          </button>
        </div>
        <p class="text-xs text-foreground-secondary mt-2">
          当前范围：
          {timeRange() === "today"
            ? "今天"
            : timeRange() === "week"
            ? "最近7天"
            : timeRange() === "month"
            ? "最近30天"
            : "全部"}
          {selectedApps().length > 0 && ` | 应用: ${selectedApps().join(", ")}`}
        </p>
      </div>
    </div>
  );
};

export default Chat;
