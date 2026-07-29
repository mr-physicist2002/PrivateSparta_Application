import {
  useCallback,
  useRef,
  useState,
  type ReactNode,
  type UIEvent,
} from "react";

interface VirtualListProps<T> {
  items: T[];
  itemHeight: number;
  overscan?: number;
  className?: string;
  renderItem: (item: T, index: number) => ReactNode;
}

/**
 * Hand-rolled fixed-height virtualizer — no dependency, smooth at 500+ rows.
 * Renders only the visible window plus overscan.
 */
export function VirtualList<T>({
  items,
  itemHeight,
  overscan = 6,
  className,
  renderItem,
}: VirtualListProps<T>) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(600);

  const onScroll = useCallback((e: UIEvent<HTMLDivElement>) => {
    setScrollTop(e.currentTarget.scrollTop);
  }, []);

  const measure = useCallback((el: HTMLDivElement | null) => {
    (containerRef as React.MutableRefObject<HTMLDivElement | null>).current = el;
    if (el) setViewport(el.clientHeight);
  }, []);

  const first = Math.max(0, Math.floor(scrollTop / itemHeight) - overscan);
  const last = Math.min(
    items.length,
    Math.ceil((scrollTop + viewport) / itemHeight) + overscan,
  );
  const visible = items.slice(first, last);

  return (
    <div
      ref={measure}
      onScroll={onScroll}
      className={`overflow-y-auto ${className ?? ""}`}
    >
      <div style={{ height: items.length * itemHeight, position: "relative" }}>
        <div
          style={{
            position: "absolute",
            top: first * itemHeight,
            left: 0,
            right: 0,
          }}
        >
          {visible.map((item, i) => (
            <div key={first + i} style={{ height: itemHeight }}>
              {renderItem(item, first + i)}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
