import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { GithubReviewHooks } from './GithubReviewHooks'
const api=vi.hoisted(()=>({getGithubReviewHooks:vi.fn(),listAutonomousAgents:vi.fn(),registerGithubReviewHook:vi.fn(),disconnectGithubReviewHook:vi.fn()}))
const auth=vi.hoisted(()=>({permissions:[] as string[]}))
vi.mock('../../../api/client',()=>({createClient:()=>api}))
vi.mock('../../../auth/AuthContext',()=>({useAuth:()=>({session:{user:{permissions:auth.permissions}}})}))
function show(permissions:string[]){auth.permissions=permissions;render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><GithubReviewHooks /></QueryClientProvider>)}
beforeEach(()=>{vi.clearAllMocks();api.getGithubReviewHooks.mockResolvedValue({login:'cli-account',hooks:[]});api.listAutonomousAgents.mockResolvedValue([{id:'reviewer',name:'API reviewer',template_key:'github_pr_reviewer',status:'enabled',validation_status:'valid'}]);api.registerGithubReviewHook.mockResolvedValue({id:'hook',login:'cli-account'})})
describe('GitHub requested review hooks',()=>{
 it('uses an optional setup token without changing the reviewer and clears it afterwards',async()=>{show(['autonomous_agent:read','factory_policy:write','autonomous_agent:manage_connectors']);const user=userEvent.setup();await screen.findByText('cli-account');await user.selectOptions(screen.getByLabelText('Enabled PR reviewer agent'),'reviewer');await user.clear(screen.getByLabelText('Public HTTPS backend URL'));await user.type(screen.getByLabelText('Public HTTPS backend URL'),'https://api.example.com');const input=screen.getByLabelText('Webhook administration token (optional)');expect(input).toHaveAttribute('type','password');await user.type(input,'setup-only-fixture');await user.click(screen.getByRole('button',{name:'Connect GitHub webhook'}));await waitFor(()=>expect(api.registerGithubReviewHook).toHaveBeenCalledWith('reviewer','https://api.example.com','setup-only-fixture'));await waitFor(()=>expect(input).toHaveValue(''));expect(screen.getByText('cli-account')).toBeInTheDocument()})

 it('does not contact the CLI without permission',()=>{show([]);expect(api.getGithubReviewHooks).not.toHaveBeenCalled()})
 it('shows the backend identity without granting setup to read-only users',async()=>{show(['autonomous_agent:read']);expect(await screen.findByText('cli-account')).toBeInTheDocument();expect(screen.queryByRole('button',{name:'Connect GitHub webhook'})).not.toBeInTheDocument()})
 it('registers the selected reviewer repository without requesting a token in the browser',async()=>{show(['autonomous_agent:read','factory_policy:write','autonomous_agent:manage_connectors']);const user=userEvent.setup();await screen.findByText('cli-account');await user.selectOptions(screen.getByLabelText('Enabled PR reviewer agent'),'reviewer');await user.clear(screen.getByLabelText('Public HTTPS backend URL'));await user.type(screen.getByLabelText('Public HTTPS backend URL'),'https://api.example.com');await user.click(screen.getByRole('button',{name:'Connect GitHub webhook'}));await waitFor(()=>expect(api.registerGithubReviewHook).toHaveBeenCalledWith('reviewer','https://api.example.com',undefined));expect(screen.getByLabelText('Webhook administration token (optional)')).not.toBeRequired()})
})
