import { StyledSelect } from '@/components/ui/Select/StyledSelect'
import { codexSupported } from '../isolation'

export type TaskExecutor = 'claude' | 'nexus' | 'codex' | 'pi'

/** Keep unsupported executors discoverable without permitting invalid runs. */
export function TaskExecutorSelect({ value, template, onChange }: {
  value: TaskExecutor
  template: string
  onChange: (value: TaskExecutor) => void
}) {
  const codeTemplate = codexSupported(template)
  return (
    <StyledSelect aria-label="Task executor" value={value} onValueChange={value => onChange(value as TaskExecutor)}>
      <option value="claude">Claude pure</option>
      <option value="nexus">Nexus harness (OpenShell)</option>
      <option value="pi" disabled={!codeTemplate} data-description={codeTemplate ? 'ChatGPT OAuth · local worker' : 'Available for issue resolvers and PR reviewers only'}>
        Pi (ChatGPT account, local worker)
      </option>
      <option value="codex" disabled={!codeTemplate} data-description={codeTemplate ? 'OpenAI API key · sandbox only' : 'Available for issue resolvers and PR reviewers only'}>
        Codex (OpenAI API key, sandbox only)
      </option>
    </StyledSelect>
  )
}
