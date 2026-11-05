// Trusted backend-only helper. stdout is a private pipe to the proxy, never logs.
import { ModelRuntime } from '/usr/local/lib/node_modules/@earendil-works/pi-coding-agent/dist/index.js'
const runtime=await ModelRuntime.create({refreshOnCreate:false})
const result=await runtime.getAuth('openai-codex')
const token=result?.auth?.apiKey
if(typeof token!=='string'||!token)process.exit(1)
const claims=JSON.parse(Buffer.from(token.split('.')[1],'base64url').toString())
const account=claims['https://api.openai.com/auth']?.chatgpt_account_id
if(typeof account!=='string'||!account)process.exit(1)
process.stdout.write(JSON.stringify({token,account}))
