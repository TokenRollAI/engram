# AI 管道设计

## 管道概览

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Engram AI Pipeline                               │
└─────────────────────────────────────────────────────────────────────────┘

              输入                      处理                      输出
         ┌──────────┐            ┌──────────────┐           ┌──────────┐
   图像 ─►│          │───────────►│              │──────────►│ OCR 文本 │
         │          │            │  PaddleOCR   │           └──────────┘
         │          │            │  (检测+识别)  │
         │  ONNX    │            └──────────────┘
         │ Runtime  │            ┌──────────────┐           ┌──────────┐
   文本 ─►│          │───────────►│   MiniLM     │──────────►│ 文本向量 │
         │          │            │  (嵌入)       │           │ (384d)   │
         │          │            └──────────────┘           └──────────┘
         │          │            ┌──────────────┐           ┌──────────┐
   图像 ─►│          │───────────►│    CLIP      │──────────►│ 视觉向量 │
         │          │            │  (嵌入)       │           │ (512d)   │
         └──────────┘            └──────────────┘           └──────────┘
                                       │
                                       │ (可选)
                                       ▼
                                ┌──────────────┐           ┌──────────┐
                                │  NLI 分类    │──────────►│ 违规判定 │
                                │  (DeBERTa)   │           └──────────┘
                                └──────────────┘
```

## 模型清单

| 模型 | 用途 | 格式 | 大小 | 输入 | 输出 |
|------|-----|------|------|-----|------|
| PP-OCRv4-det | 文本检测 | ONNX INT8 | 4MB | 图像 | 文本框坐标 |
| PP-OCRv4-rec | 文本识别 | ONNX INT8 | 10MB | 裁剪图像 | 文本字符串 |
| all-MiniLM-L6-v2 | 文本嵌入 | ONNX | 80MB | 文本 | 384d 向量 |
| CLIP-ViT-B-32 | 视觉嵌入 | ONNX | 350MB | 图像 | 512d 向量 |
| DeBERTa-v3-xsmall-NLI | 零样本分类 | ONNX | 70MB | 文本对 | 蕴含概率 |
| Qwen-2.5-7B-Instruct | 摘要生成 | GGUF Q4_K_M | 4.5GB | 提示词 | 文本 |

## OCR 管道详细设计

### 输入预处理

```rust
fn preprocess_for_ocr(image: &RgbaImage) -> DynamicImage {
    // 1. 下采样到标准分辨率 (如果超过 1920x1080)
    let resized = if image.width() > 1920 || image.height() > 1080 {
        image.resize(1920, 1080, FilterType::Triangle)
    } else {
        image.clone()
    };

    // 2. 转换为 RGB (去除 Alpha)
    let rgb = resized.to_rgb8();

    // 3. 归一化到 [0, 1] (模型要求)
    // 在 ONNX 输入时处理

    rgb
}
```

### 检测阶段

```
输入: RGB 图像 (H x W x 3)
      ↓
转换: NCHW 格式 (1 x 3 x H x W), float32, 归一化
      ↓
推理: PP-OCRv4-det.onnx
      ↓
输出: 概率图 (H x W)
      ↓
后处理: DB 后处理算法
      - 二值化 (阈值 0.3)
      - 膨胀操作
      - 轮廓检测
      - 最小外接矩形
      ↓
结果: Vec<BoundingBox>
```

### 识别阶段

```
对每个检测到的文本框:
      ↓
输入: 裁剪并旋转的文本行图像
      ↓
调整: 固定高度 48px，宽度按比例
      ↓
推理: PP-OCRv4-rec.onnx
      ↓
输出: 字符概率序列
      ↓
解码: CTC 解码 (贪心或 Beam Search)
      ↓
结果: (text: String, confidence: f32)
```

### 执行提供者配置

```rust
use ort::{Environment, SessionBuilder, ExecutionProvider};

fn create_ocr_session(model_path: &str) -> Result<Session> {
    let env = Environment::builder()
        .with_name("engram_ocr")
        .build()?;

    let mut builder = SessionBuilder::new(&env)?;

    // 按优先级尝试不同的执行提供者
    #[cfg(target_os = "macos")]
    {
        builder = builder.with_execution_providers([
            ExecutionProvider::CoreML(Default::default()),
            ExecutionProvider::CPU(Default::default()),
        ])?;
    }

    #[cfg(target_os = "windows")]
    {
        builder = builder.with_execution_providers([
            ExecutionProvider::DirectML(Default::default()),
            ExecutionProvider::OpenVINO(Default::default()),
            ExecutionProvider::CPU(Default::default()),
        ])?;
    }

    #[cfg(target_os = "linux")]
    {
        builder = builder.with_execution_providers([
            ExecutionProvider::CUDA(Default::default()),
            ExecutionProvider::CPU(Default::default()),
        ])?;
    }

    builder.commit_from_file(model_path)
}
```

## 嵌入管道设计

### 文本嵌入

```rust
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

struct TextEmbedder {
    model: TextEmbedding,
}

impl TextEmbedder {
    fn new() -> Result<Self> {
        let model = TextEmbedding::try_new(InitOptions {
            model_name: EmbeddingModel::AllMiniLML6V2,
            show_download_progress: true,
            ..Default::default()
        })?;
        Ok(Self { model })
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // 截断过长文本 (MiniLM 最大 256 tokens)
        let truncated = truncate_text(text, 256);

        let embeddings = self.model.embed(vec![truncated], None)?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.model.embed(texts.to_vec(), None)
    }
}
```

### 视觉嵌入

```rust
struct ImageEmbedder {
    session: Session,
    processor: ClipProcessor,
}

impl ImageEmbedder {
    fn embed(&self, image: &RgbaImage) -> Result<Vec<f32>> {
        // 1. CLIP 预处理
        //    - Resize 到 224x224
        //    - Center crop
        //    - Normalize: mean=[0.48145466, 0.4578275, 0.40821073]
        //                 std=[0.26862954, 0.26130258, 0.27577711]
        let input = self.processor.preprocess(image)?;

        // 2. 推理
        let outputs = self.session.run(ort::inputs![input]?)?;

        // 3. L2 归一化
        let embedding = outputs[0].try_extract::<f32>()?;
        Ok(l2_normalize(embedding))
    }
}
```

## 语义黑名单 (零样本分类)

### 工作原理

零样本分类利用自然语言推理 (NLI) 模型，判断输入文本是否"蕴含"某个标签描述。

```
输入:
  premise = "账户余额: ¥12,345.67 转账记录..."
  hypothesis = "这段内容涉及银行账户信息"

NLI 模型输出:
  entailment: 0.95   ← 高置信度表示匹配
  neutral: 0.03
  contradiction: 0.02
```

### 实现

```rust
struct SemanticFilter {
    session: Session,
    tokenizer: Tokenizer,
    blacklist_descriptions: Vec<String>,
    threshold: f32,
}

impl SemanticFilter {
    fn should_block(&self, text: &str) -> bool {
        for description in &self.blacklist_descriptions {
            // 构建 NLI 输入对
            let premise = text;
            let hypothesis = description;

            // Tokenize
            let encoding = self.tokenizer.encode(
                (premise, hypothesis),
                true
            ).unwrap();

            // 推理
            let scores = self.run_nli(&encoding);

            // 检查蕴含分数
            if scores.entailment > self.threshold {
                return true;  // 匹配黑名单
            }
        }
        false
    }
}
```

### 默认语义黑名单

```toml
# settings.toml
[semantic_blacklist]
descriptions = [
    "涉及个人隐私的聊天内容",
    "银行账户或信用卡信息",
    "密码或身份认证凭据",
    "医疗健康敏感信息",
]
threshold = 0.85
```

## LLM 摘要生成

### Sidecar 架构

```
Engram 主进程
     │
     │ 启动子进程
     ▼
┌────────────────────────────────────────────┐
│  llama-server                              │
│  --model /path/to/qwen-2.5-7b-q4_k_m.gguf │
│  --host 127.0.0.1                          │
│  --port {random}                           │
│  --ctx-size 4096                           │
│  --n-gpu-layers 35  (如果有 GPU)           │
└────────────────────────────────────────────┘
     ▲
     │ HTTP POST /completion
     │
Engram 摘要模块
```

### Prompt 模板

```
<|im_start|>system
你是一个专业的数字活动分析师。根据用户的屏幕活动日志生成结构化摘要。
输出必须是有效的 JSON 格式。
<|im_end|>
<|im_start|>user
以下是过去 15 分钟的屏幕活动记录：

[09:00:15] Visual Studio Code - main.rs
OCR: impl ScreenCapture for Windows { ... }

[09:02:30] Chrome - Rust scap crate documentation
OCR: scap is a cross-platform screen capture library...

[09:05:45] Terminal - cargo build
OCR: Compiling engram v0.1.0 ...

请生成 JSON 格式的摘要，包含以下字段：
- summary: 200字以内的活动总结
- topics: 主题标签数组
- entities: 提取的实体 [{name, type}]
- links: 出现的 URL 数组
<|im_end|>
<|im_start|>assistant
```

### 响应解析

```rust
#[derive(Deserialize)]
struct SummaryResponse {
    summary: String,
    topics: Vec<String>,
    entities: Vec<Entity>,
    links: Vec<String>,
}

#[derive(Deserialize)]
struct Entity {
    name: String,
    #[serde(rename = "type")]
    entity_type: String,
}

async fn generate_summary(
    client: &HttpClient,
    traces: &[Trace],
) -> Result<SummaryResponse> {
    let prompt = build_prompt(traces);

    let response = client.post(&format!("{}/completion", llama_url))
        .json(&json!({
            "prompt": prompt,
            "max_tokens": 1024,
            "temperature": 0.3,
            "grammar": SUMMARY_JSON_GRAMMAR,  // GBNF 语法约束
        }))
        .send()
        .await?;

    let completion: CompletionResponse = response.json().await?;
    let summary: SummaryResponse = serde_json::from_str(&completion.content)?;

    Ok(summary)
}
```

### GBNF 语法约束

```gbnf
root ::= "{" ws summary-kv "," ws topics-kv "," ws entities-kv "," ws links-kv ws "}"

summary-kv ::= "\"summary\"" ws ":" ws string
topics-kv ::= "\"topics\"" ws ":" ws "[" ws (string ("," ws string)*)? ws "]"
entities-kv ::= "\"entities\"" ws ":" ws "[" ws (entity ("," ws entity)*)? ws "]"
links-kv ::= "\"links\"" ws ":" ws "[" ws (string ("," ws string)*)? ws "]"

entity ::= "{" ws "\"name\"" ws ":" ws string "," ws "\"type\"" ws ":" ws string ws "}"

string ::= "\"" ([^"\\] | "\\" .)* "\""
ws ::= [ \t\n]*
```

## 性能优化策略

### 1. 模型预热

```rust
impl AiPipeline {
    async fn warmup(&self) {
        // OCR 预热 (第一次推理较慢)
        let dummy_image = RgbaImage::new(224, 224);
        let _ = self.ocr.process(&dummy_image);

        // 嵌入模型预热
        let _ = self.text_embedder.embed("warmup");
    }
}
```

### 2. 批处理

```rust
// 每 10 帧进行一次批量嵌入
const EMBEDDING_BATCH_SIZE: usize = 10;

impl EmbeddingQueue {
    fn enqueue(&mut self, text: String, callback: Callback) {
        self.queue.push((text, callback));

        if self.queue.len() >= EMBEDDING_BATCH_SIZE {
            self.flush();
        }
    }

    fn flush(&mut self) {
        let texts: Vec<_> = self.queue.iter().map(|(t, _)| t.clone()).collect();
        let embeddings = self.embedder.embed_batch(&texts).unwrap();

        for ((_, callback), emb) in self.queue.drain(..).zip(embeddings) {
            callback(emb);
        }
    }
}
```

### 3. 模型量化配置

| 硬件配置 | OCR 精度 | 嵌入精度 | LLM 量化 |
|---------|---------|---------|---------|
| 高端 (16GB+, GPU) | FP16 | FP32 | Q8_0 |
| 中端 (8-16GB) | INT8 | FP32 | Q4_K_M |
| 低端 (<8GB) | INT8 | FP16 | Q4_0 或禁用 |
