import { Button, Menu, MenuItemRadio, MenuList, MenuPopover, MenuTrigger } from '@fluentui/react-components';
import { Icon } from './Icon.tsx';

export function ChoiceMenu({
  label,
  value,
  choices,
  disabled = false,
  onSelect,
  className = '',
}: {
  label: string;
  value: string;
  choices: readonly { value: string; label: string; disabled?: boolean }[];
  disabled?: boolean;
  onSelect(value: string): void;
  className?: string;
}) {
  const selected = choices.find(choice => choice.value === value)?.label || value;
  return (
    <Menu>
      <MenuTrigger disableButtonEnhancement>
        <Button
          size="small"
          className={`li-shell-choice ${className}`}
          disabled={disabled || !choices.length}
          aria-label={`${label}: ${selected}`}
        >
          <span>{selected}</span>
          <Icon name="chevron-down" />
        </Button>
      </MenuTrigger>
      <MenuPopover data-react-owned="true" className="li-shell-choice-menu">
        <MenuList aria-label={label} checkedValues={{ choice: [value] }}>
          {choices.map(choice => (
            <MenuItemRadio
              key={choice.value}
              name="choice"
              value={choice.value}
              disabled={disabled || choice.disabled}
              onClick={() => onSelect(choice.value)}
            >
              {choice.label}
            </MenuItemRadio>
          ))}
        </MenuList>
      </MenuPopover>
    </Menu>
  );
}
