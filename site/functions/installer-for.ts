// The installers live in the product repo, so they always match the latest release.
const source = 'https://raw.githubusercontent.com/ArynBuilds/lightsout/main/installer/'
const kept = new Map<string, { text: string; at: number }>()

async function installer(file: string): Promise<string | undefined> {
  const hit = kept.get(file)
  if (hit && Date.now() - hit.at < 5 * 60 * 1000) return hit.text
  try {
    const r = await fetch(source + file)
    if (!r.ok) return hit?.text
    const text = await r.text()
    kept.set(file, { text, at: Date.now() })
    return text
  } catch {
    return hit?.text
  }
}

/** The installer a request asks for: by its address, or at the root by who asks (PowerShell or curl). */
export async function installerFor(request: Request): Promise<Response | undefined> {
  const { pathname } = new URL(request.url)
  const agent = (request.headers.get('user-agent') ?? '').toLowerCase()
  let file: string | undefined
  if (pathname === '/install.ps1') file = 'install.ps1'
  else if (pathname === '/install.sh') file = 'install.sh'
  else if (pathname === '/' && agent.includes('powershell')) file = 'install.ps1'
  else if (pathname === '/' && /^(curl|wget)/.test(agent)) file = 'install.sh'
  if (!file) return undefined
  const text = await installer(file)
  if (text === undefined) return new Response('Lights Out: the installer is not reachable right now. Try again in a minute.\n', { status: 503 })
  return new Response(request.method === 'HEAD' ? null : text, {
    headers: { 'content-type': 'text/plain; charset=utf-8', 'cache-control': 'no-cache' },
  })
}
