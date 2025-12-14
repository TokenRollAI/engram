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
    },
  },
  plugins: [],
};
