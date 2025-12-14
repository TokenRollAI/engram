import { Component, createSignal, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { format } from "date-fns";

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

const Search: Component = () => {
  const [query, setQuery] = createSignal("");
  const [results, setResults] = createSignal<SearchResult[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [searched, setSearched] = createSignal(false);

  // 执行搜索
  const doSearch = async () => {
    const q = query().trim();
    if (!q) return;

    setLoading(true);
    setSearched(true);

    try {
      const data = await invoke<SearchResult[]>("search_traces", {
        query: q,
        mode: "keyword",
        limit: 50,
      });
      setResults(data);
    } catch (e) {
      console.error("Search failed:", e);
      setResults([]);
    } finally {
      setLoading(false);
    }
  };

  // 处理回车键
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      doSearch();
    }
  };

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
              type="text"
              placeholder="搜索屏幕记忆..."
              value={query()}
              onInput={(e) => setQuery(e.currentTarget.value)}
              onKeyDown={handleKeyDown}
              class="w-full pl-10 pr-4 py-3 bg-background-card border border-gray-600 rounded-lg text-white placeholder-foreground-secondary focus:outline-none focus:ring-2 focus:ring-accent focus:border-transparent"
            />
          </div>
          <button
            onClick={doSearch}
            disabled={loading()}
            class="px-6 py-3 bg-accent hover:bg-accent-hover disabled:opacity-50 rounded-lg transition-colors"
          >
            搜索
          </button>
        </div>

        {/* 搜索选项 */}
        <div class="flex items-center space-x-4 mt-3 text-sm text-foreground-secondary">
          <label class="flex items-center space-x-2 cursor-pointer">
            <input type="radio" name="mode" value="keyword" checked class="accent-accent" />
            <span>关键词</span>
          </label>
          <label class="flex items-center space-x-2 cursor-pointer opacity-50">
            <input type="radio" name="mode" value="semantic" disabled class="accent-accent" />
            <span>语义搜索 (Phase 2)</span>
          </label>
        </div>
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
          </div>
        </Show>

        <Show when={!loading() && searched() && results().length === 0}>
          <div class="flex flex-col items-center justify-center h-full text-foreground-secondary">
            <p class="text-4xl mb-4">😔</p>
            <p>没有找到匹配的结果</p>
            <p class="text-sm mt-2">尝试使用不同的关键词</p>
          </div>
        </Show>

        <Show when={!loading() && results().length > 0}>
          <div class="space-y-4">
            <p class="text-sm text-foreground-secondary">
              找到 {results().length} 条结果
            </p>

            <For each={results()}>
              {(result) => (
                <div class="bg-background-card rounded-lg p-4 hover:ring-1 hover:ring-gray-600 transition-all">
                  <div class="flex items-start space-x-4">
                    {/* 缩略图 */}
                    <div class="w-32 h-20 bg-background-secondary rounded flex-shrink-0 flex items-center justify-center">
                      <span class="text-foreground-secondary">🖼️</span>
                    </div>

                    {/* 信息 */}
                    <div class="flex-1 min-w-0">
                      <div class="flex items-center justify-between mb-1">
                        <h3 class="font-medium truncate">
                          {result.trace.app_name || "未知应用"}
                        </h3>
                        <span class="text-xs text-foreground-secondary font-mono">
                          {format(new Date(result.trace.timestamp), "yyyy-MM-dd HH:mm")}
                        </span>
                      </div>

                      <p class="text-sm text-foreground-secondary truncate mb-2">
                        {result.trace.window_title || "-"}
                      </p>

                      <Show when={result.trace.ocr_text}>
                        <p class="text-sm bg-background p-2 rounded truncate">
                          {result.trace.ocr_text?.substring(0, 200)}
                          {(result.trace.ocr_text?.length ?? 0) > 200 && "..."}
                        </p>
                      </Show>

                      <div class="flex items-center mt-2 text-xs text-foreground-secondary">
                        <span class="px-2 py-0.5 bg-accent/20 text-accent rounded">
                          相关度: {(result.score * 100).toFixed(0)}%
                        </span>
                      </div>
                    </div>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
    </div>
  );
};

export default Search;
