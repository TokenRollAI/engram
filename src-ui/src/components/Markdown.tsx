import { Component, createEffect, createSignal } from "solid-js";
import { marked } from "marked";
import Prism from "prismjs";

// 导入常用语言支持
import "prismjs/components/prism-javascript";
import "prismjs/components/prism-typescript";
import "prismjs/components/prism-jsx";
import "prismjs/components/prism-tsx";
import "prismjs/components/prism-css";
import "prismjs/components/prism-json";
import "prismjs/components/prism-python";
import "prismjs/components/prism-rust";
import "prismjs/components/prism-bash";
import "prismjs/components/prism-sql";
import "prismjs/components/prism-yaml";
import "prismjs/components/prism-markdown";

interface MarkdownProps {
  content: string;
  class?: string;
}

// 配置 marked 渲染器
const renderer = new marked.Renderer();

// 自定义代码块渲染，添加语言类名以支持 Prism 高亮
renderer.code = ({ text, lang }: { text: string; lang?: string }) => {
  const language = lang || "text";
  const validLang = Prism.languages[language] ? language : "text";

  let highlighted: string;
  if (Prism.languages[validLang]) {
    highlighted = Prism.highlight(text, Prism.languages[validLang], validLang);
  } else {
    highlighted = text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }

  return `<pre class="language-${validLang}"><code class="language-${validLang}">${highlighted}</code></pre>`;
};

// 配置 marked
marked.setOptions({
  renderer,
  gfm: true, // 启用 GitHub Flavored Markdown
  breaks: true, // 将换行符转换为 <br>
});

const Markdown: Component<MarkdownProps> = (props) => {
  const [html, setHtml] = createSignal("");

  createEffect(() => {
    const parsed = marked.parse(props.content);
    if (typeof parsed === "string") {
      setHtml(parsed);
    } else {
      // 处理 Promise 情况（虽然同步模式下不会发生）
      parsed.then((result) => setHtml(result));
    }
  });

  return (
    <div
      class={`prose prose-sm max-w-none ${props.class || ""}`}
      innerHTML={html()}
    />
  );
};

export default Markdown;
