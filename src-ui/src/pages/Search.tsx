import { Component, createSignal, For, Show, onMount, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import { format, subDays, startOfDay, endOfDay } from "date-fns";
import { zhCN } from "date-fns/locale";

// 类型定义
interface Trace {
  id: number;
  timestamp: number;
  image_path: string | null;
  app_name: string | null;
  window_title: string | null;
  is_fullscreen: boolean;
  is_idle: boolean;
  ocr_text: string | null;
  created_at: number;
}

interface SearchResult {
  trace: Trace;
  score: number;
  highlights: { text: string; start: number; end: number }[];
}

interface AiStatus {
  vlm_ready: boolean;
  embedder_ready: boolean;
  pending_analysis_count: number;
  pending_embedding_count: number;
}

// 时间范围预设
const TIME_PRESETS = [
  { label: "全部时间", value: "all" },
  { label: "今天", value: "today" },
  { label: "昨天", value: "yesterday" },
  { label: "最近7天", value: "7days" },
  { label: "最近30天", value: "30days" },
  { label: "自定义", value: "custom" },
];

const Search: Component = () => {
  const [query, setQuery] = createSignal("");
  const [results, setResults] = createSignal<SearchResult[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [searched, setSearched] = createSignal(false);
  const [searchMode, setSearchMode] = createSignal<"keyword" | "semantic">("keyword");
  const [aiStatus, setAiStatus] = createSignal<AiStatus | null>(null);

  // 高级过滤
  const [showFilters, setShowFilters] = createSignal(false);
  const [timePreset, setTimePreset] = createSignal("all");
  const [customStartDate, setCustomStartDate] = createSignal("");
  const [customEndDate, setCustomEndDate] = createSignal("");
  const [appFilter, setAppFilter] = createSignal<string[]>([]);
  const [availableApps, setAvailableApps] = createSignal<string[]>([]);

  // 搜索历史
  const [searchHistory, setSearchHistory] = createSignal<string[]>([]);
  const [showHistory, setShowHistory] = createSignal(false);
  const [selectedHistoryIndex, setSelectedHistoryIndex] = createSignal(-1);

  // 详情弹窗
  const [selectedResult, setSelectedResult] = createSignal<SearchResult | null>(null);
  const [selectedImageSrc, setSelectedImageSrc] = createSignal<string | null>(null);

  // 搜索框引用
  let searchInputRef: HTMLInputElement | undefined;

  // 加载 AI 状态
  const loadAiStatus = async () => {
    try {
      const status = await invoke<AiStatus>("get_ai_status");
      setAiStatus(status);
    } catch (e) {
      console.error("Failed to load AI status:", e);
    }
  };

  // 加载搜索历史（从 localStorage）
  const loadSearchHistory = () => {
    try {
      const history = localStorage.getItem("engram_search_history");
      if (history) {
        setSearchHistory(JSON.parse(history));
      }
    } catch (e) {
      console.error("Failed to load search history:", e);
    }
  };

  // 保存搜索历史
  const saveSearchHistory = (newQuery: string) => {
    const history = searchHistory();
    const filtered = history.filter(h => h !== newQuery);
    const updated = [newQuery, ...filtered].slice(0, 10); // 最多保留10条
    setSearchHistory(updated);
    localStorage.setItem("engram_search_history", JSON.stringify(updated));
  };

  // 计算时间范围
  const getTimeRange = (): { start: number | null; end: number | null } => {
    const now = new Date();
    switch (timePreset()) {
      case "today":
        return { start: startOfDay(now).getTime(), end: endOfDay(now).getTime() };
      case "yesterday":
        const yesterday = subDays(now, 1);
        return { start: startOfDay(yesterday).getTime(), end: endOfDay(yesterday).getTime() };
      case "7days":
        return { start: subDays(now, 7).getTime(), end: now.getTime() };
      case "30days":
        return { start: subDays(now, 30).getTime(), end: now.getTime() };
      case "custom":
        return {
          start: customStartDate() ? new Date(customStartDate()).getTime() : null,
          end: customEndDate() ? endOfDay(new Date(customEndDate())).getTime() : null,
        };
      default:
        return { start: null, end: null };
    }
  };

  // 执行搜索
  const doSearch = async () => {
    const q = query().trim();
    if (!q) return;

    setLoading(true);
    setSearched(true);
    setShowHistory(false);
    saveSearchHistory(q);

    try {
      const { start, end } = getTimeRange();
      const data = await invoke<SearchResult[]>("search_traces", {
        query: q,
        mode: searchMode(),
        startTime: start,
        endTime: end,
        appFilter: appFilter().length > 0 ? appFilter() : null,
        limit: 50,
      });
      setResults(data);

      // 收集可用的应用列表
      const apps = new Set<string>();
      data.forEach(r => {
        if (r.trace.app_name) apps.add(r.trace.app_name);
      });
      setAvailableApps(Array.from(apps).sort());
    } catch (e) {
      console.error("Search failed:", e);
      setResults([]);
    } finally {
      setLoading(false);
    }
  };

  // 处理键盘事件
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      if (showHistory() && selectedHistoryIndex() >= 0) {
        const selected = searchHistory()[selectedHistoryIndex()];
        setQuery(selected);
        setShowHistory(false);
        setSelectedHistoryIndex(-1);
        doSearch();
      } else {
        doSearch();
      }
    } else if (e.key === "ArrowDown" && showHistory()) {
      e.preventDefault();
      setSelectedHistoryIndex(i => Math.min(i + 1, searchHistory().length - 1));
    } else if (e.key === "ArrowUp" && showHistory()) {
      e.preventDefault();
      setSelectedHistoryIndex(i => Math.max(i - 1, -1));
    } else if (e.key === "Escape") {
      setShowHistory(false);
      setSelectedHistoryIndex(-1);
    }
  };

  // 全局快捷键 Ctrl+K
  const handleGlobalKeyDown = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "k") {
      e.preventDefault();
      searchInputRef?.focus();
      searchInputRef?.select();
    }
  };

  // 高亮匹配文本
  const highlightText = (text: string, query: string): string => {
    if (!query || !text) return text;
    const regex = new RegExp(`(${query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi');
    return text.replace(regex, '<mark class="bg-accent/40 text-white rounded px-0.5">$1</mark>');
  };

  // 获取文本片段（上下文）
  const getTextSnippet = (text: string | null, query: string, maxLen: number = 200): string => {
    if (!text) return "";
    const lowerText = text.toLowerCase();
    const lowerQuery = query.toLowerCase();
    const index = lowerText.indexOf(lowerQuery);

    if (index === -1) {
      return text.substring(0, maxLen) + (text.length > maxLen ? "..." : "");
    }

    const start = Math.max(0, index - 50);
    const end = Math.min(text.length, index + query.length + 150);
    let snippet = text.substring(start, end);

    if (start > 0) snippet = "..." + snippet;
    if (end < text.length) snippet = snippet + "...";

    return snippet;
  };

  // 获取图片源
  const getImageSrc = async (relativePath: string): Promise<string | null> => {
    try {
      const fullPath = await invoke<string>("get_image_path", { relativePath });
      return convertFileSrc(fullPath);
    } catch (e) {
      console.error("Failed to get image path:", e);
      return null;
    }
  };

  // 打开详情
  const openDetail = async (result: SearchResult) => {
    setSelectedResult(result);
    if (result.trace.image_path) {
      const src = await getImageSrc(result.trace.image_path);
      setSelectedImageSrc(src);
    } else {
      setSelectedImageSrc(null);
    }
  };

  // 关闭详情
  const closeDetail = () => {
    setSelectedResult(null);
    setSelectedImageSrc(null);
  };

  // 切换应用过滤
  const toggleAppFilter = (app: string) => {
    setAppFilter(prev => {
      if (prev.includes(app)) {
        return prev.filter(a => a !== app);
      } else {
        return [...prev, app];
      }
    });
  };

  // 清除历史
  const clearHistory = () => {
    setSearchHistory([]);
    localStorage.removeItem("engram_search_history");
  };

  onMount(() => {
    loadAiStatus();
    loadSearchHistory();
    window.addEventListener("keydown", handleGlobalKeyDown);
  });

  onCleanup(() => {
    window.removeEventListener("keydown", handleGlobalKeyDown);
  });

  return (
    <div class="h-full flex flex-col">
      {/* 搜索栏 */}
      <header class="px-6 py-4 border-b border-gray-700">
        <div class="flex items-center space-x-4">
          <div class="flex-1 relative">
            <span class="absolute left-3 top-1/2 -translate-y-1/2 text-foreground-secondary">
              🔍
            </span>
            <input
              ref={searchInputRef}
              type="text"
              placeholder="搜索屏幕记忆... (Ctrl+K)"
              value={query()}
              onInput={(e) => {
                setQuery(e.currentTarget.value);
                setShowHistory(e.currentTarget.value.length === 0 && searchHistory().length > 0);
              }}
              onFocus={() => {
                if (query().length === 0 && searchHistory().length > 0) {
                  setShowHistory(true);
                }
              }}
              onBlur={() => {
                // 延迟关闭以允许点击历史项
                setTimeout(() => setShowHistory(false), 200);
              }}
              onKeyDown={handleKeyDown}
              class="w-full pl-10 pr-4 py-3 bg-background-card border border-gray-600 rounded-lg text-white placeholder-foreground-secondary focus:outline-none focus:ring-2 focus:ring-accent focus:border-transparent"
            />

            {/* 搜索历史下拉 */}
            <Show when={showHistory() && searchHistory().length > 0}>
              <div class="absolute top-full left-0 right-0 mt-1 bg-background-card border border-gray-600 rounded-lg shadow-xl z-50 overflow-hidden">
                <div class="flex items-center justify-between px-3 py-2 border-b border-gray-700">
                  <span class="text-xs text-foreground-secondary">搜索历史</span>
                  <button
                    onClick={(e) => { e.stopPropagation(); clearHistory(); }}
                    class="text-xs text-foreground-secondary hover:text-white"
                  >
                    清除
                  </button>
                </div>
                <For each={searchHistory()}>
                  {(item, index) => (
                    <div
                      class={`px-3 py-2 cursor-pointer transition-colors ${
                        selectedHistoryIndex() === index()
                          ? "bg-accent text-white"
                          : "hover:bg-gray-700"
                      }`}
                      onClick={() => {
                        setQuery(item);
                        setShowHistory(false);
                        doSearch();
                      }}
                    >
                      <span class="text-sm">{item}</span>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>

          <button
            onClick={() => setShowFilters(!showFilters())}
            class={`px-4 py-3 rounded-lg transition-colors ${
              showFilters() ? "bg-accent" : "bg-background-card border border-gray-600 hover:bg-gray-700"
            }`}
            title="高级过滤"
          >
            ⚙️
          </button>

          <button
            onClick={doSearch}
            disabled={loading()}
            class="px-6 py-3 bg-accent hover:bg-accent-hover disabled:opacity-50 rounded-lg transition-colors"
          >
            搜索
          </button>
        </div>

        {/* 搜索模式选择 */}
        <div class="flex items-center space-x-4 mt-3 text-sm text-foreground-secondary">
          <label class="flex items-center space-x-2 cursor-pointer">
            <input
              type="radio"
              name="mode"
              value="keyword"
              checked={searchMode() === "keyword"}
              onChange={() => setSearchMode("keyword")}
              class="accent-accent"
            />
            <span>关键词搜索</span>
          </label>
          <label
            class={`flex items-center space-x-2 ${
              aiStatus()?.embedder_ready ? "cursor-pointer" : "opacity-50 cursor-not-allowed"
            }`}
            title={aiStatus()?.embedder_ready ? "使用向量嵌入进行语义搜索" : "嵌入模型未就绪"}
          >
            <input
              type="radio"
              name="mode"
              value="semantic"
              checked={searchMode() === "semantic"}
              onChange={() => aiStatus()?.embedder_ready && setSearchMode("semantic")}
              disabled={!aiStatus()?.embedder_ready}
              class="accent-accent"
            />
            <span>语义搜索</span>
            <Show when={aiStatus()?.embedder_ready}>
              <span class="text-xs text-success">●</span>
            </Show>
          </label>
        </div>

        {/* 高级过滤面板 */}
        <Show when={showFilters()}>
          <div class="mt-4 p-4 bg-background rounded-lg space-y-4">
            {/* 时间范围 */}
            <div>
              <label class="block text-sm text-foreground-secondary mb-2">时间范围</label>
              <div class="flex flex-wrap gap-2">
                <For each={TIME_PRESETS}>
                  {(preset) => (
                    <button
                      onClick={() => setTimePreset(preset.value)}
                      class={`px-3 py-1 text-sm rounded transition-colors ${
                        timePreset() === preset.value
                          ? "bg-accent text-white"
                          : "bg-background-card hover:bg-gray-700"
                      }`}
                    >
                      {preset.label}
                    </button>
                  )}
                </For>
              </div>

              {/* 自定义日期 */}
              <Show when={timePreset() === "custom"}>
                <div class="flex items-center gap-4 mt-3">
                  <input
                    type="date"
                    value={customStartDate()}
                    onInput={(e) => setCustomStartDate(e.currentTarget.value)}
                    class="px-3 py-2 bg-background-card border border-gray-600 rounded text-sm"
                  />
                  <span class="text-foreground-secondary">至</span>
                  <input
                    type="date"
                    value={customEndDate()}
                    onInput={(e) => setCustomEndDate(e.currentTarget.value)}
                    class="px-3 py-2 bg-background-card border border-gray-600 rounded text-sm"
                  />
                </div>
              </Show>
            </div>

            {/* 应用过滤 */}
            <Show when={availableApps().length > 0}>
              <div>
                <label class="block text-sm text-foreground-secondary mb-2">应用过滤</label>
                <div class="flex flex-wrap gap-2">
                  <For each={availableApps()}>
                    {(app) => (
                      <button
                        onClick={() => toggleAppFilter(app)}
                        class={`px-3 py-1 text-sm rounded transition-colors ${
                          appFilter().includes(app)
                            ? "bg-accent text-white"
                            : "bg-background-card hover:bg-gray-700"
                        }`}
                      >
                        {app}
                      </button>
                    )}
                  </For>
                </div>
              </div>
            </Show>
          </div>
        </Show>
      </header>

      {/* 搜索结果 */}
      <div class="flex-1 overflow-auto p-6">
        <Show when={loading()}>
          <div class="flex items-center justify-center h-full">
            <p class="text-foreground-secondary">搜索中...</p>
          </div>
        </Show>

        <Show when={!loading() && !searched()}>
          <div class="flex flex-col items-center justify-center h-full text-foreground-secondary">
            <p class="text-4xl mb-4">🔍</p>
            <p>输入关键词搜索你的屏幕记忆</p>
            <p class="text-sm mt-2">支持搜索 OCR 提取的文本和窗口标题</p>
            <p class="text-xs mt-4 text-foreground-secondary/60">
              按 <kbd class="px-1.5 py-0.5 bg-background-card rounded text-xs">Ctrl</kbd>
              {" + "}
              <kbd class="px-1.5 py-0.5 bg-background-card rounded text-xs">K</kbd>
              {" 快速聚焦搜索框"}
            </p>
          </div>
        </Show>

        <Show when={!loading() && searched() && results().length === 0}>
          <div class="flex flex-col items-center justify-center h-full text-foreground-secondary">
            <p class="text-4xl mb-4">😔</p>
            <p>没有找到匹配的结果</p>
            <p class="text-sm mt-2">尝试使用不同的关键词或调整过滤条件</p>
          </div>
        </Show>

        <Show when={!loading() && results().length > 0}>
          <div class="space-y-4">
            <div class="flex items-center justify-between">
              <p class="text-sm text-foreground-secondary">
                找到 {results().length} 条结果
                <Show when={searchMode() === "semantic"}>
                  <span class="ml-2 text-accent">(语义搜索)</span>
                </Show>
              </p>
              <Show when={appFilter().length > 0}>
                <button
                  onClick={() => setAppFilter([])}
                  class="text-xs text-foreground-secondary hover:text-white"
                >
                  清除过滤
                </button>
              </Show>
            </div>

            <For each={results()}>
              {(result) => (
                <div
                  class="bg-background-card rounded-lg p-4 hover:ring-1 hover:ring-accent/50 transition-all cursor-pointer"
                  onClick={() => openDetail(result)}
                >
                  <div class="flex items-start space-x-4">
                    {/* 缩略图 */}
                    <div class="w-40 h-24 bg-background-secondary rounded flex-shrink-0 flex items-center justify-center overflow-hidden">
                      <Show
                        when={result.trace.image_path}
                        fallback={<span class="text-foreground-secondary">🖼️</span>}
                      >
                        <ResultThumbnail imagePath={result.trace.image_path!} />
                      </Show>
                    </div>

                    {/* 信息 */}
                    <div class="flex-1 min-w-0">
                      <div class="flex items-center justify-between mb-1">
                        <h3 class="font-medium truncate">
                          {result.trace.app_name || "未知应用"}
                        </h3>
                        <div class="flex items-center space-x-3">
                          <span class="px-2 py-0.5 bg-accent/20 text-accent rounded text-xs">
                            {(result.score * 100).toFixed(0)}%
                          </span>
                          <span class="text-xs text-foreground-secondary font-mono">
                            {format(new Date(result.trace.timestamp), "yyyy-MM-dd HH:mm", { locale: zhCN })}
                          </span>
                        </div>
                      </div>

                      <p class="text-sm text-foreground-secondary truncate mb-2">
                        {result.trace.window_title || "-"}
                      </p>

                      <Show when={result.trace.ocr_text}>
                        <p
                          class="text-sm bg-background p-2 rounded line-clamp-2"
                          innerHTML={highlightText(
                            getTextSnippet(result.trace.ocr_text, query()),
                            query()
                          )}
                        />
                      </Show>
                    </div>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>

      {/* 详情弹窗 */}
      <Show when={selectedResult()}>
        <div
          class="fixed inset-0 bg-black/80 flex items-center justify-center z-50"
          onClick={closeDetail}
        >
          <div
            class="bg-background-secondary rounded-lg max-w-5xl max-h-[90vh] overflow-auto m-4"
            onClick={(e) => e.stopPropagation()}
          >
            {/* 图像预览 */}
            <div class="relative bg-background">
              <Show
                when={selectedImageSrc()}
                fallback={
                  <div class="aspect-video flex items-center justify-center text-foreground-secondary">
                    无图像
                  </div>
                }
              >
                <img
                  src={selectedImageSrc()!}
                  alt="Screenshot"
                  class="w-full h-auto max-h-[60vh] object-contain"
                />
              </Show>
            </div>

            {/* 详细信息 */}
            <div class="p-6">
              <div class="flex items-center justify-between mb-4">
                <h3 class="text-lg font-semibold">
                  {selectedResult()?.trace.app_name || "未知应用"}
                </h3>
                <span class="px-3 py-1 bg-accent/20 text-accent rounded">
                  相关度: {((selectedResult()?.score ?? 0) * 100).toFixed(0)}%
                </span>
              </div>

              <dl class="space-y-2 text-sm">
                <div class="flex">
                  <dt class="w-24 text-foreground-secondary">时间</dt>
                  <dd>
                    {selectedResult() &&
                      format(
                        new Date(selectedResult()!.trace.timestamp),
                        "yyyy-MM-dd HH:mm:ss"
                      )}
                  </dd>
                </div>
                <div class="flex">
                  <dt class="w-24 text-foreground-secondary">窗口标题</dt>
                  <dd class="flex-1 break-all">{selectedResult()?.trace.window_title || "-"}</dd>
                </div>
                <Show when={selectedResult()?.trace.ocr_text}>
                  <div>
                    <dt class="text-foreground-secondary mb-1">OCR 文本</dt>
                    <dd
                      class="bg-background p-3 rounded text-xs max-h-60 overflow-auto whitespace-pre-wrap"
                      innerHTML={highlightText(selectedResult()?.trace.ocr_text || "", query())}
                    />
                  </div>
                </Show>
              </dl>

              <button
                class="mt-6 w-full py-2 bg-accent hover:bg-accent-hover rounded transition-colors"
                onClick={closeDetail}
              >
                关闭
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
};

// 结果缩略图组件
const ResultThumbnail: Component<{ imagePath: string }> = (props) => {
  const [src, setSrc] = createSignal<string | null>(null);
  const [error, setError] = createSignal(false);

  onMount(async () => {
    try {
      const fullPath = await invoke<string>("get_image_path", { relativePath: props.imagePath });
      setSrc(convertFileSrc(fullPath));
    } catch {
      setError(true);
    }
  });

  return (
    <Show
      when={src() && !error()}
      fallback={<span class="text-foreground-secondary text-xs">加载失败</span>}
    >
      <img
        src={src()!}
        alt=""
        class="w-full h-full object-cover"
        onError={() => setError(true)}
      />
    </Show>
  );
};

export default Search;
