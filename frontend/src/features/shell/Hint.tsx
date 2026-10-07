import { Tooltip } from '@fluentui/react-components';
import type { ReactElement } from 'react';

/** Fluent tooltip for icon-only buttons and shortcut hints. As a `label` it
 * names the control with the same text it shows; as a `description` it adds
 * detail (such as a shortcut) without replacing the control's name. */
export function Hint({
  content,
  relationship = 'label',
  enabled = true,
  children,
}: {
  content: string;
  relationship?: 'label' | 'description';
  enabled?: boolean;
  children: ReactElement;
}) {
  if (!enabled) return children;
  return (
    <Tooltip content={content} relationship={relationship} withArrow>
      {children}
    </Tooltip>
  );
}

// Lets Fluent triggers (MenuTrigger, PopoverTrigger) reach the wrapped button
// through this wrapper, as they do through Tooltip itself.
(Hint as typeof Hint & { isFluentTriggerComponent: boolean }).isFluentTriggerComponent = true;
