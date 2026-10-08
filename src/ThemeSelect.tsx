import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";

export type ThemeSelectOption = {
  value: string;
  label: string;
  disabled?: boolean;
};

type ThemeSelectProps = {
  value: string;
  options: readonly ThemeSelectOption[];
  onChange: (value: string) => void;
  ariaLabel?: string;
  disabled?: boolean;
};

function firstEnabledIndex(options: readonly ThemeSelectOption[]) {
  return options.findIndex((option) => !option.disabled);
}

export function ThemeSelect({ value, options, onChange, ariaLabel, disabled = false }: ThemeSelectProps) {
  const [open, setOpen] = useState(false);
  const [highlightedIndex, setHighlightedIndex] = useState(-1);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuId = useId();
  const selectedIndex = options.findIndex((option) => option.value === value);
  const selectedOption = selectedIndex >= 0 ? options[selectedIndex] : undefined;

  useEffect(() => {
    if (!open) return;
    const closeOnOutside = (event: PointerEvent) => {
      if (event.target instanceof Node && !rootRef.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutside);
    return () => document.removeEventListener("pointerdown", closeOnOutside);
  }, [open]);

  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);

  function openMenu() {
    if (!options.length) return;
    const nextIndex = selectedIndex >= 0 && !options[selectedIndex]?.disabled ? selectedIndex : firstEnabledIndex(options);
    setHighlightedIndex(nextIndex);
    setOpen(true);
  }

  function moveHighlight(direction: 1 | -1) {
    if (!options.length) return;
    let index = highlightedIndex >= 0 ? highlightedIndex : selectedIndex;
    for (let attempt = 0; attempt < options.length; attempt += 1) {
      index = ((index + direction) % options.length + options.length) % options.length;
      if (!options[index]?.disabled) {
        setHighlightedIndex(index);
        return;
      }
    }
  }

  function commitOption(index: number) {
    const option = options[index];
    if (!option || option.disabled) return;
    onChange(option.value);
    setOpen(false);
    triggerRef.current?.focus();
  }

  function handleKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key === "Tab") {
      setOpen(false);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      setOpen(false);
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowRight") {
      event.preventDefault();
      if (!open) openMenu();
      else moveHighlight(1);
      return;
    }
    if (event.key === "ArrowUp" || event.key === "ArrowLeft") {
      event.preventDefault();
      if (!open) openMenu();
      else moveHighlight(-1);
      return;
    }
    if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      if (!open) setOpen(true);
      const direction = event.key === "Home" ? 1 : -1;
      let nextIndex = -1;
      for (let index = 0; index < options.length; index += 1) {
        const candidate = direction === 1 ? index : options.length - 1 - index;
        if (!options[candidate]?.disabled) {
          nextIndex = candidate;
          break;
        }
      }
      setHighlightedIndex(nextIndex);
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (!open) openMenu();
      else commitOption(highlightedIndex >= 0 ? highlightedIndex : selectedIndex);
    }
  }

  return <div className="theme-select" ref={rootRef}>
    <button
      ref={triggerRef}
      type="button"
      className={`theme-select-trigger${open ? " theme-select-trigger-open" : ""}`}
      role="combobox"
      aria-label={ariaLabel}
      aria-haspopup="listbox"
      aria-expanded={open}
      aria-controls={menuId}
      aria-activedescendant={open && highlightedIndex >= 0 ? `${menuId}-option-${highlightedIndex}` : undefined}
      disabled={disabled}
      onClick={() => { if (open) setOpen(false); else openMenu(); }}
      onKeyDown={handleKeyDown}
    >
      <span className="theme-select-trigger-label">{selectedOption?.label ?? "Select an option"}</span>
      <span className="theme-select-chevron" aria-hidden="true"><svg viewBox="0 0 16 16" focusable="false"><path d="m4 6 4 4 4-4" /></svg></span>
    </button>
    {open && <div id={menuId} className="theme-select-menu" role="listbox" aria-label={ariaLabel}>
      {options.map((option, index) => <button
        key={`${option.value}-${index}`}
        id={`${menuId}-option-${index}`}
        type="button"
        role="option"
        className="theme-select-option"
        data-highlighted={index === highlightedIndex ? "true" : undefined}
        data-selected={index === selectedIndex ? "true" : undefined}
        aria-selected={index === selectedIndex}
        disabled={option.disabled}
        onMouseDown={(event) => event.preventDefault()}
        onMouseEnter={() => !option.disabled && setHighlightedIndex(index)}
        onClick={() => commitOption(index)}
      >
        <span className="theme-select-option-label">{option.label}</span>
        {index === selectedIndex && <span className="theme-select-option-check" aria-hidden="true"><svg viewBox="0 0 16 16" focusable="false"><path d="m3.2 8.4 3.1 3.1 6.5-7" /></svg></span>}
      </button>)}
    </div>}
  </div>;
}
