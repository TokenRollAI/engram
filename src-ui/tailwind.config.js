/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // 主背景色
        background: {
          DEFAULT: "#1a1a1a",
          secondary: "#2d2d2d",
          card: "#3d3d3d",
        },
        // 文字颜色
        foreground: {
          DEFAULT: "#ffffff",
          secondary: "#a0a0a0",
        },
        // 强调色
        accent: {
          DEFAULT: "#3b82f6",
          hover: "#2563eb",
        },
        // 状态色
        success: "#22c55e",
        warning: "#eab308",
        error: "#ef4444",
      },
      fontFamily: {
        sans: [
          "-apple-system",
          "BlinkMacSystemFont",
          "Segoe UI",
          "Roboto",
          "Helvetica Neue",
          "Arial",
          "Noto Sans SC",
          "sans-serif",
        ],
        mono: [
          "ui-monospace",
          "SFMono-Regular",
          "SF Mono",
          "Menlo",
          "Consolas",
          "monospace",
        ],
      },
      // 自定义 typography 样式以匹配深色主题
      typography: {
        DEFAULT: {
          css: {
            color: "#ffffff",
            a: {
              color: "#3b82f6",
              "&:hover": {
                color: "#2563eb",
              },
            },
            strong: {
              color: "#ffffff",
            },
            code: {
              color: "#e5e7eb",
              backgroundColor: "#374151",
              padding: "0.125rem 0.25rem",
              borderRadius: "0.25rem",
              fontWeight: "400",
            },
            "code::before": {
              content: '""',
            },
            "code::after": {
              content: '""',
            },
            pre: {
              backgroundColor: "#1f2937",
              color: "#e5e7eb",
            },
            h1: {
              color: "#ffffff",
            },
            h2: {
              color: "#ffffff",
            },
            h3: {
              color: "#ffffff",
            },
            h4: {
              color: "#ffffff",
            },
            blockquote: {
              color: "#a0a0a0",
              borderLeftColor: "#3b82f6",
            },
            hr: {
              borderColor: "#4b5563",
            },
            ol: {
              li: {
                "&::marker": {
                  color: "#a0a0a0",
                },
              },
            },
            ul: {
              li: {
                "&::marker": {
                  color: "#a0a0a0",
                },
              },
            },
            th: {
              color: "#ffffff",
            },
            td: {
              color: "#e5e7eb",
            },
          },
        },
      },
    },
  },
  plugins: [require("@tailwindcss/typography")],
};
