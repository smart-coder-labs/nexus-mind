import { screen } from '@testing-library/react'
import type { UserEvent } from '@testing-library/user-event'

/** Select an item from the Radix-backed select used by the admin UI. */
export async function chooseSelectOption(user: UserEvent, trigger: HTMLElement, name: string | RegExp) {
  await user.click(trigger)
  if (trigger.getAttribute('aria-expanded') !== 'true') await user.keyboard('{ArrowDown}')
  await user.click(await screen.findByRole('option', { name }))
}
