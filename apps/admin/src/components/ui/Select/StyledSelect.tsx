import * as React from 'react'
import { cn } from '@/lib/utils'
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectTrigger, SelectValue } from './Select'

type NativeOptionProps = React.OptionHTMLAttributes<HTMLOptionElement> & {
  'data-description'?: string
  'data-color'?: string
  'data-avatar-src'?: string
}
type NativeOption = React.ReactElement<NativeOptionProps>
type NativeGroup = React.ReactElement<React.OptgroupHTMLAttributes<HTMLOptGroupElement>>

export interface StyledSelectProps extends Omit<React.SelectHTMLAttributes<HTMLSelectElement>, 'onChange' | 'value' | 'defaultValue' | 'size' | 'multiple'> {
  value?: string | number
  defaultValue?: string | number
  onChange?: React.ChangeEventHandler<HTMLSelectElement>
  onValueChange?: (value: string) => void
  placeholder?: string
  'data-testid'?: string
}

function optionValue(option: NativeOption) {
  return String(option.props.value ?? option.props.children ?? '')
}

function renderOption(option: NativeOption) {
  const value = optionValue(option)
  const description = option.props['data-description']
  const indicatorColor = option.props['data-color']
  const avatarSrc = option.props['data-avatar-src']
  return <SelectItem key={value} value={value} disabled={option.props.disabled} description={description} indicatorColor={indicatorColor} avatarSrc={avatarSrc}>{option.props.children}</SelectItem>
}

function renderOptions(children: React.ReactNode): React.ReactNode[] {
  const result: React.ReactNode[] = []
  React.Children.toArray(children).forEach((child, index) => {
    if (!React.isValidElement(child)) return
    if (child.type === React.Fragment) { result.push(...renderOptions((child.props as { children?: React.ReactNode }).children)); return }
    if (child.type === 'option') { result.push(renderOption(child as NativeOption)); return }
    if (child.type === 'optgroup') {
      const group = child as NativeGroup
      result.push(<SelectGroup key={`${group.props.label ?? 'group'}-${index}`}>
        {group.props.label && <SelectLabel>{group.props.label}</SelectLabel>}
        {renderOptions(group.props.children)}
      </SelectGroup>)
    }
  })
  return result
}

/** A Radix/shadcn select that keeps the familiar native select/option call site. */
export const StyledSelect = React.forwardRef<HTMLButtonElement, StyledSelectProps>(function StyledSelect(
  { value, defaultValue, onChange, onValueChange, children, className, disabled, required, name, form, id, placeholder, autoFocus, tabIndex, title, style, onBlur, onFocus, onKeyDown, onKeyUp, onClick, 'data-testid': dataTestId, ...ariaProps },
  ref,
) {
  const [uncontrolledValue, setUncontrolledValue] = React.useState(String(defaultValue ?? ''))
  const selectedValue = String(value ?? uncontrolledValue)

  const handleValueChange = (nextValue: string) => {
    if (value === undefined) setUncontrolledValue(nextValue)
    onValueChange?.(nextValue)
    if (onChange) {
      const target = { value: nextValue, name } as HTMLSelectElement
      onChange({ target, currentTarget: target } as React.ChangeEvent<HTMLSelectElement>)
    }
  }

  return <Select value={selectedValue} onValueChange={handleValueChange} disabled={disabled} required={required} name={name}>
    <SelectTrigger
      ref={ref}
      id={id}
      form={form}
      disabled={disabled}
      autoFocus={autoFocus}
      tabIndex={tabIndex}
      title={title}
      style={style}
      onBlur={onBlur as unknown as React.FocusEventHandler<HTMLButtonElement>}
      onFocus={onFocus as unknown as React.FocusEventHandler<HTMLButtonElement>}
      onKeyDown={onKeyDown as unknown as React.KeyboardEventHandler<HTMLButtonElement>}
      onKeyUp={onKeyUp as unknown as React.KeyboardEventHandler<HTMLButtonElement>}
      onClick={onClick as unknown as React.MouseEventHandler<HTMLButtonElement>}
      aria-label={ariaProps['aria-label']}
      aria-describedby={ariaProps['aria-describedby']}
      aria-invalid={ariaProps['aria-invalid']}
      aria-labelledby={ariaProps['aria-labelledby']}
      data-testid={dataTestId}
      className={cn('h-9 min-w-0 max-w-full justify-between rounded-md border border-input bg-background px-3 text-sm text-foreground shadow-xs transition-colors', className)}
    >
      <SelectValue placeholder={placeholder} />
    </SelectTrigger>
    <SelectContent align="start" className="max-w-[min(26rem,calc(100vw-2rem))]">
      {renderOptions(children)}
    </SelectContent>
  </Select>
})
