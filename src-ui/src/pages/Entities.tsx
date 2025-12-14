import { Component, createSignal, onMount, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

// 类型定义
interface Entity {
  id: number;
  name: string;
  type: string;
  mention_count: number;
  first_seen: number;
  last_seen: number;
  metadata: string | null;
}

interface Trace {
  id: number;
  timestamp: number;
  image_path: string | null;
  app_name: string | null;
  window_title: string | null;
  ocr_text: string | null;
  activity_session_id?: number | null;
  is_key_action?: boolean;
}

const Entities: Component = () => {
  const [entities, setEntities] = createSignal<Entity[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [selectedEntity, setSelectedEntity] = createSignal<Entity | null>(null);
  const [relatedTraces, setRelatedTraces] = createSignal<Trace[]>([]);
  const [entityType, setEntityType] = createSignal<string>("all");
  const [searchQuery, setSearchQuery] = createSignal("");
  const [orderByMentions, setOrderByMentions] = createSignal(true);

  // 获取实体列表
  const fetchEntities = async () => {
    setLoading(true);
    try {
      const type = entityType() === "all" ? undefined : entityType();

      const result = await invoke<Entity[]>("get_entities", {
        entityType: type,
        limit: 200,
        orderByMentions: orderByMentions(),
      });
      setEntities(result);
    } catch (e) {
      console.error("Failed to fetch entities:", e);
    } finally {
      setLoading(false);
    }
  };

  // 搜索实体
  const searchEntities = async () => {
    if (!searchQuery().trim()) {
      fetchEntities();
      return;
    }

    setLoading(true);
    try {
      const result = await invoke<Entity[]>("search_entities", {
        query: searchQuery(),
        limit: 100,
      });
      setEntities(result);
    } catch (e) {
      console.error("Failed to search entities:", e);
    } finally {
      setLoading(false);
    }
  };

  // 获取实体关联的痕迹
  const fetchRelatedTraces = async (entityId: number) => {
    try {
      const result = await invoke<Trace[]>("get_traces_by_entity", {
        entityId,
        limit: 20,
      });
      setRelatedTraces(result);
    } catch (e) {
      console.error("Failed to fetch related traces:", e);
      setRelatedTraces([]);
    }
  };

  // 删除实体
  const deleteEntity = async (id: number) => {
    if (!confirm("确定要删除这个实体吗？")) return;

    try {
      await invoke("delete_entity", { id });
      setEntities(entities().filter((e) => e.id !== id));
      if (selectedEntity()?.id === id) {
        setSelectedEntity(null);
        setRelatedTraces([]);
      }
    } catch (e) {
      console.error("Failed to delete entity:", e);
    }
  };

  // 选择实体
  const selectEntity = async (entity: Entity) => {
    setSelectedEntity(entity);
    await fetchRelatedTraces(entity.id);
  };

  // 格式化时间
  const formatTime = (timestamp: number) => {
    return new Date(timestamp).toLocaleString("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  // 格式化相对时间
  const formatRelativeTime = (timestamp: number) => {
    const now = Date.now();
    const diff = now - timestamp;

    if (diff < 60000) return "刚刚";
    if (diff < 3600000) return `${Math.floor(diff / 60000)} 分钟前`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)} 小时前`;
    if (diff < 604800000) return `${Math.floor(diff / 86400000)} 天前`;

    return new Date(timestamp).toLocaleDateString("zh-CN");
  };

  // 获取实体类型标签
  const getEntityTypeLabel = (type: string) => {
    switch (type) {
      case "person":
        return "人物";
      case "project":
        return "项目";
      case "technology":
        return "技术";
      case "url":
        return "链接";
      case "file":
        return "文件";
      default:
        return type;
    }
  };

  // 获取实体类型颜色
  const getEntityTypeColor = (type: string) => {
    switch (type) {
      case "person":
        return "bg-green-500/20 text-green-400 border-green-500/30";
      case "project":
        return "bg-blue-500/20 text-blue-400 border-blue-500/30";
      case "technology":
        return "bg-orange-500/20 text-orange-400 border-orange-500/30";
      case "url":
        return "bg-cyan-500/20 text-cyan-400 border-cyan-500/30";
      case "file":
        return "bg-yellow-500/20 text-yellow-400 border-yellow-500/30";
      default:
        return "bg-gray-500/20 text-gray-400 border-gray-500/30";
    }
  };

  // 获取实体类型图标
  const getEntityTypeIcon = (type: string) => {
    switch (type) {
      case "person":
        return "👤";
      case "project":
        return "📁";
      case "technology":
        return "⚙️";
      case "url":
        return "🔗";
      case "file":
        return "📄";
      default:
        return "📌";
    }
  };

  onMount(() => {
    fetchEntities();
  });

  return (
    <div class="h-full flex flex-col p-4 overflow-hidden">
      {/* 头部 */}
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-xl font-bold">知识实体</h2>

        <div class="flex items-center gap-4">
          {/* 搜索框 */}
          <div class="flex items-center gap-2">
            <input
              type="text"
              value={searchQuery()}
              onInput={(e) => setSearchQuery(e.target.value)}
              onKeyPress={(e) => e.key === "Enter" && searchEntities()}
              placeholder="搜索实体..."
              class="px-3 py-2 bg-background-card border border-gray-700 rounded-lg text-sm w-48"
            />
            <button
              onClick={searchEntities}
              class="px-3 py-2 bg-accent hover:bg-accent/80 rounded-lg text-sm transition-colors"
            >
              搜索
            </button>
          </div>

          {/* 类型过滤 */}
          <select
            value={entityType()}
            onChange={(e) => {
              setEntityType(e.target.value);
              setSearchQuery("");
              fetchEntities();
            }}
            class="px-3 py-2 bg-background-card border border-gray-700 rounded-lg text-sm"
          >
            <option value="all">全部类型</option>
            <option value="person">人物</option>
            <option value="project">项目</option>
            <option value="technology">技术</option>
            <option value="url">链接</option>
            <option value="file">文件</option>
          </select>

          {/* 排序方式 */}
          <select
            value={orderByMentions() ? "mentions" : "recent"}
            onChange={(e) => {
              setOrderByMentions(e.target.value === "mentions");
              fetchEntities();
            }}
            class="px-3 py-2 bg-background-card border border-gray-700 rounded-lg text-sm"
          >
            <option value="mentions">按提及次数</option>
            <option value="recent">按最近出现</option>
          </select>
        </div>
      </div>

      {/* 内容区 */}
      <div class="flex-1 flex gap-4 overflow-hidden">
        {/* 实体列表 */}
        <div class="w-1/2 overflow-y-auto pr-2">
          <Show
            when={!loading()}
            fallback={
              <div class="text-center py-8 text-foreground-secondary">
                加载中...
              </div>
            }
          >
            <Show
              when={entities().length > 0}
              fallback={
                <div class="text-center py-8 text-foreground-secondary">
                  暂无实体数据
                </div>
              }
            >
              <div class="grid grid-cols-2 gap-3">
                <For each={entities()}>
                  {(entity) => (
                    <div
                      onClick={() => selectEntity(entity)}
                      class={`p-3 bg-background-card rounded-lg cursor-pointer transition-all hover:ring-2 hover:ring-accent border ${
                        selectedEntity()?.id === entity.id
                          ? "ring-2 ring-accent"
                          : "border-transparent"
                      }`}
                    >
                      <div class="flex items-start justify-between mb-2">
                        <div class="flex items-center gap-2">
                          <span class="text-lg">
                            {getEntityTypeIcon(entity.type)}
                          </span>
                          <span class="font-medium truncate max-w-[120px]">
                            {entity.name}
                          </span>
                        </div>
                        <span
                          class={`px-2 py-0.5 rounded text-xs ${getEntityTypeColor(
                            entity.type
                          )}`}
                        >
                          {getEntityTypeLabel(entity.type)}
                        </span>
                      </div>

                      <div class="flex items-center justify-between text-xs text-foreground-secondary">
                        <span>提及 {entity.mention_count} 次</span>
                        <span>{formatRelativeTime(entity.last_seen)}</span>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </Show>
        </div>

        {/* 实体详情 */}
        <div class="w-1/2 overflow-y-auto bg-background-card rounded-lg p-4">
          <Show
            when={selectedEntity()}
            fallback={
              <div class="h-full flex items-center justify-center text-foreground-secondary">
                选择一个实体查看详情
              </div>
            }
          >
            {(entity) => (
              <div class="space-y-4">
                {/* 实体信息 */}
                <div class="flex items-start justify-between">
                  <div class="flex items-center gap-3">
                    <span class="text-3xl">
                      {getEntityTypeIcon(entity().type)}
                    </span>
                    <div>
                      <h3 class="text-lg font-bold">{entity().name}</h3>
                      <span
                        class={`px-2 py-0.5 rounded text-xs ${getEntityTypeColor(
                          entity().type
                        )}`}
                      >
                        {getEntityTypeLabel(entity().type)}
                      </span>
                    </div>
                  </div>

                  <button
                    onClick={() => deleteEntity(entity().id)}
                    class="text-error hover:text-error/80 text-sm"
                  >
                    删除
                  </button>
                </div>

                {/* 统计信息 */}
                <div class="grid grid-cols-2 gap-4">
                  <div class="bg-background/50 rounded-lg p-3">
                    <p class="text-xs text-foreground-secondary mb-1">提及次数</p>
                    <p class="text-2xl font-bold">{entity().mention_count}</p>
                  </div>
                  <div class="bg-background/50 rounded-lg p-3">
                    <p class="text-xs text-foreground-secondary mb-1">首次出现</p>
                    <p class="text-sm">{formatTime(entity().first_seen)}</p>
                  </div>
                </div>

                <div class="bg-background/50 rounded-lg p-3">
                  <p class="text-xs text-foreground-secondary mb-1">最近出现</p>
                  <p class="text-sm">{formatTime(entity().last_seen)}</p>
                </div>

                {/* 关联的痕迹 */}
                <div>
                  <h4 class="text-sm font-semibold text-foreground-secondary mb-3">
                    相关记录 ({relatedTraces().length})
                  </h4>

                  <Show
                    when={relatedTraces().length > 0}
                    fallback={
                      <p class="text-sm text-foreground-secondary">
                        暂无关联记录
                      </p>
                    }
                  >
                    <div class="space-y-2 max-h-60 overflow-y-auto">
                      <For each={relatedTraces()}>
                        {(trace) => (
                          <div class="bg-background/50 rounded-lg p-3">
                            <div class="flex items-center justify-between mb-1">
                              <span class="text-sm font-medium truncate max-w-[200px]">
                                {trace.app_name || "未知应用"}
                              </span>
                              <span class="text-xs text-foreground-secondary">
                                {formatRelativeTime(trace.timestamp)}
                              </span>
                            </div>
                            <p class="text-xs text-foreground-secondary truncate">
                              {trace.window_title || trace.ocr_text?.slice(0, 50) || "无标题"}
                            </p>
                          </div>
                        )}
                      </For>
                    </div>
                  </Show>
                </div>
              </div>
            )}
          </Show>
        </div>
      </div>
    </div>
  );
};

export default Entities;
