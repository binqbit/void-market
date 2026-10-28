import { Moon, Sun } from "lucide-react";
import { useEffect, useState } from "react";

type Theme = "light" | "dark";

function savedTheme(): Theme {
  try {
    const saved = localStorage.getItem("void.theme");
    return saved === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

export function ThemeControl() {
  const [theme, setTheme] = useState(savedTheme);
  const nextTheme = theme === "dark" ? "light" : "dark";
  const label = `Switch to ${nextTheme} theme`;

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute(
        "content",
        getComputedStyle(document.documentElement)
          .getPropertyValue("--bg")
          .trim(),
      );
  }, [theme]);

  function toggleTheme() {
    setTheme(nextTheme);
    try {
      localStorage.setItem("void.theme", nextTheme);
    } catch {
      // Theme still works for this page when browser storage is unavailable.
    }
  }

  return (
    <button
      className="theme-toggle"
      type="button"
      aria-label={label}
      title={label}
      onClick={toggleTheme}
    >
      {theme === "dark" ? (
        <Sun size={18} aria-hidden="true" />
      ) : (
        <Moon size={18} aria-hidden="true" />
      )}
    </button>
  );
}
