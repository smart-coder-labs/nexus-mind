import { beforeEach, describe, expect, it, vi } from 'vitest'
import { NexusMindClient } from './client'

const fetchMock=vi.fn()

beforeEach(()=>{
  fetchMock.mockReset()
  vi.stubGlobal('fetch',fetchMock)
  vi.stubGlobal('window',{location:{replace:vi.fn()}})
})

describe('NexusMindClient autonomous agent contracts',()=>{
  it('creates disabled managed agents and never adds role authority',async()=>{
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify({id:'a1',status:'disabled',revision:{revision:1}}),{status:201}))
    const client=new NexusMindClient('https://api.test')
    await client.createAutonomousAgent({name:'Daily QA',template_key:'qa',config:{outputs:['nexusmind']},budgets:{wall_time_seconds:300}})
    const [url,options]=fetchMock.mock.calls[0]
    expect(url).toBe('https://api.test/v1/autonomous-agents')
    expect(options.method).toBe('POST')
    expect(JSON.parse(options.body)).not.toHaveProperty('role')
  })

  it('sends connector secrets only in the write request and uses operational endpoints',async()=>{
    fetchMock
      .mockResolvedValueOnce(new Response(JSON.stringify({id:'c1',secret_configured:true}),{status:200}))
      .mockResolvedValueOnce(new Response(JSON.stringify({id:'r1',status:'cancelled'}),{status:200}))
      .mockResolvedValueOnce(new Response(JSON.stringify({id:'d1',status:'pending'}),{status:200}))
    const client=new NexusMindClient('')
    await client.putAutonomousAgentConnector({kind:'slack',name:'alerts',secret:'https://hooks.slack.com/services/secret',metadata:{},scopes:[]})
    await client.cancelAutonomousAgentRun('r1')
    await client.retryAutonomousAgentDelivery('d1')
    expect(fetchMock.mock.calls[0][0]).toBe('/v1/autonomous-agent-connectors')
    expect(fetchMock.mock.calls[1][0]).toBe('/v1/autonomous-agent-runs/r1/cancel')
    expect(fetchMock.mock.calls[2][0]).toBe('/v1/autonomous-agent-deliveries/d1/retry')
  })
})
