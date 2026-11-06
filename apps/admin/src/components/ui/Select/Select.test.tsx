import { useState } from 'react'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from './Select'
function Example(){
 const [value,setValue]=useState('')
 return <><Select value={value} onValueChange={setValue}><SelectTrigger aria-label="Project"><SelectValue/></SelectTrigger><SelectContent><SelectItem value="">All projects</SelectItem><SelectItem value="demo">Demo</SelectItem><SelectItem value="disabled" disabled>Unavailable</SelectItem></SelectContent></Select><output aria-label="Selected value">{value || 'empty'}</output></>
}
describe('Select compatibility with existing filters',()=>{
 it('supports empty options and keyboard selection without changing the API value',async()=>{
  HTMLElement.prototype.scrollIntoView = ()=>{}
  const user=userEvent.setup(); render(<Example/>);
  const trigger=screen.getByRole('combobox',{name:'Project'});
  await waitFor(()=>expect(trigger).toHaveTextContent('All projects'));
  trigger.focus(); await user.keyboard('{Enter}');
  await user.keyboard('{ArrowDown}{Enter}');
  await waitFor(()=>expect(screen.getByLabelText('Selected value')).toHaveTextContent('demo'));
  await user.keyboard('{Enter}{Home}{Enter}');
  await waitFor(()=>expect(screen.getByLabelText('Selected value')).toHaveTextContent('empty'));
  expect(trigger).toHaveFocus();
 }, 20000)
})
