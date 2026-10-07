import { Select } from '@fluentui/react-components';

/** The application's picker: Fluent's Select, fed by a plain choice list so
 * callers describe options once and every picker looks and behaves the same. */
export function ChoiceSelect({
  id,
  label,
  value,
  choices,
  disabled = false,
  onSelect,
  className,
}: {
  id?: string;
  label: string;
  value: string;
  choices: readonly { value: string; label: string; disabled?: boolean }[];
  disabled?: boolean;
  onSelect(value: string): void;
  className?: string;
}) {
  const known = choices.some(choice => choice.value === value);
  return (
    <Select
      id={id}
      size="small"
      className={className}
      aria-label={label}
      value={value}
      disabled={disabled || !choices.length}
      onChange={(_, data) => onSelect(data.value)}
    >
      {!known && value && <option value={value}>{value}</option>}
      {!choices.length && <option value="">No choices available</option>}
      {choices.map(choice => (
        <option key={choice.value} value={choice.value} disabled={choice.disabled}>
          {choice.label}
        </option>
      ))}
    </Select>
  );
}
