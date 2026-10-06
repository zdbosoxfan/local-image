import { useLayoutEffect, useRef, useState, type RefObject } from 'react';

const px = (value: string) => parseFloat(value) || 0;

function contentWidth(node: HTMLElement) {
  const children = [...node.children];
  const gaps = px(getComputedStyle(node).columnGap) * Math.max(0, children.length - 1);
  return children.reduce((sum, child) => sum + child.getBoundingClientRect().width, gaps);
}

/** Keeps toolbar labels while the toolbar fits on one line beside its siblings
 * in `bar`, and switches to icon-only buttons (same accessible names) when it
 * would wrap. The labelled width is remembered so the choice does not flicker,
 * and is measured again when the interface density changes. */
export function useCompactToolbar(bar: RefObject<HTMLElement | null>, toolbar: RefObject<HTMLElement | null>) {
  const [compact, setCompact] = useState(false);
  const labelledWidth = useRef(0);
  useLayoutEffect(() => {
    const barNode = bar.current,
      toolbarNode = toolbar.current;
    if (!barNode || !toolbarNode) return;
    const measure = () => {
      const style = getComputedStyle(barNode);
      const siblings = [...barNode.children].filter(child => child !== toolbarNode);
      const used = siblings.reduce(
        (sum, child) => sum + child.getBoundingClientRect().width + px(style.columnGap),
        px(style.paddingLeft) + px(style.paddingRight),
      );
      if (!compact) labelledWidth.current = contentWidth(toolbarNode);
      const next = labelledWidth.current > barNode.clientWidth - used + 0.5;
      if (next !== compact) setCompact(next);
    };
    measure();
    const resize = new ResizeObserver(measure);
    resize.observe(barNode);
    resize.observe(toolbarNode);
    const density = new MutationObserver(() => (compact ? setCompact(false) : measure()));
    density.observe(document.documentElement, { attributes: true, attributeFilter: ['data-ui-density'] });
    return () => {
      resize.disconnect();
      density.disconnect();
    };
  }, [bar, toolbar, compact]);
  return compact;
}
