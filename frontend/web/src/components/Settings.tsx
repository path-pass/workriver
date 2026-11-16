// Settings: the few choices that belong to the person, not to the work on screen.
import { useEffect, useState } from 'react'
import { Settings as Gear } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { MOCK } from '@/data'
import { machines as loadMachines, notifyWanted, record, setMachine, wantNotify } from '@/runtime'
import type { Machine, Workspace } from '@/types'
import { LANGS, useT, type Lang } from '@/i18n'
import { readTheme, setTheme, THEMES, type Theme } from '@/theme'

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <span className="text-sm">{label}</span>
      {children}
    </div>
  )
}

/**
 * The other machines: add one (a name and its ssh target), follow it or not, see whether its
 * connection is up and why not; and link a workspace over there to one here.
 */
function Machines({ ws }: { ws: Workspace | null }) {
  const { t } = useT()
  const [list, setList] = useState<Machine[]>([])
  const [error, setError] = useState('')
  const [name, setName] = useState('')
  const [ssh, setSsh] = useState('')
  const fail = (e: unknown) => setError(t('machines.failed', { error: String(e) }))
  const act = (p: Promise<Machine[]>) => p.then(l => { setList(l); setError('') }, fail)
  // The list as the service has it; what is live about each comes with the data.
  useEffect(() => { loadMachines().then(setList, () => {}) }, [ws?.machines?.length])
  const live = (m: Machine) => ws?.machines?.find(x => x.label === m.label) ?? m
  const state = (m: Machine) => !m.enabled ? t('machines.off') : m.connecting ? t('machines.connecting') : m.tunnel && m.reachable ? t('machines.connected', { count: m.agents }) : m.tunnel ? t('machines.noHerdr') : t('machines.down')
  const here = (ws?.workspaces ?? []).filter(g => !g.machine)
  const there = (ws?.workspaces ?? []).filter(g => g.machine)
  const link = (machine: string, workspace: string, to: string, was?: string) => {
    const undo = was ? record('workspace.unlinked', { machine, workspace }) : Promise.resolve()
    undo.then(() => (to ? record('workspace.linked', { machine, workspace, local: to }) : undefined)).then(() => setError(''), fail)
  }
  const field = 'w-0 min-w-0 flex-1 rounded-sm border border-line bg-surface px-2 py-1 text-sm'
  return (
    <section aria-labelledby="machines" className="flex flex-col gap-2 border-t border-line pt-3">
      <h3 id="machines" className="text-sm font-semibold">{t('machines.title')}</h3>
      <p className="text-xs text-ink-2">{list.length ? t('machines.lead') : t('machines.none')}</p>
      {list.map(live).map(m => (
        <div key={m.label} className="flex flex-col gap-0.5">
          <div className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate text-sm">{m.label} <span className="font-mono text-xs text-ink-2">{m.ssh}</span></span>
            <span className="flex-none text-xs text-ink-2">{state(m)}</span>
            <Switch checked={m.enabled} aria-label={t('machines.follow', { machine: m.label })} onCheckedChange={on => act(setMachine(m.label, on ? 'enable' : 'disable'))} />
            <Button variant="ghost" size="icon-sm" aria-label={t('machines.remove', { machine: m.label })} onClick={() => act(setMachine(m.label, 'remove'))}>×</Button>
          </div>
          {m.enabled && (m.tunnelError || m.error) && <p className="text-xs wrap-anywhere text-ink-2">{m.tunnelError ?? m.error}</p>}
          {m.enabled && String(m.port) !== location.port && <p className="text-xs text-ink-2">{t('machines.port', { port: m.port })}</p>}
        </div>
      ))}
      <form className="flex gap-1.5" onSubmit={e => { e.preventDefault(); act(setMachine(name.trim(), 'add', ssh.trim()).then(l => { setName(''); setSsh(''); return l })) }}>
        <input className={field} value={name} onChange={e => setName(e.target.value)} placeholder={t('machines.name')} aria-label={t('machines.name')} />
        <input className={field} value={ssh} onChange={e => setSsh(e.target.value)} placeholder={t('machines.ssh')} aria-label={t('machines.ssh')} />
        <Button type="submit" size="sm" variant="outline" disabled={!name.trim() || !ssh.trim()}>{t('machines.add')}</Button>
      </form>
      {there.length > 0 && (
        <>
          <h4 className="pt-1 text-xs font-semibold">{t('machines.links')}</h4>
          <p className="text-xs text-ink-2">{t('machines.linkLead')}</p>
          {there.map(g => (
            <label key={g.id} className="flex items-center gap-2 text-sm">
              <span className="min-w-0 flex-1 truncate"><span className="font-mono text-xs text-ink-2">{g.machine}:</span>{g.label}</span>
              <select className="max-w-[45%] rounded-sm border border-line bg-surface px-1 py-0.5 text-sm" aria-label={t('machines.linkTo', { machine: g.machine, workspace: g.label })} value={g.linkedTo ?? ''} onChange={e => link(g.machine!, g.label, e.target.value, g.linkedTo)}>
                <option value="">{t('machines.notLinked')}</option>
                {/* A linked workspace that is not open here now still shows what it is linked to. */}
                {[...new Set([...here.map(h => h.label), ...(g.linkedTo ? [g.linkedTo] : [])])].map(l => <option key={l} value={l}>{l}</option>)}
              </select>
            </label>
          ))}
        </>
      )}
      {error && <p role="alert" className="text-xs wrap-anywhere text-you-ink">{error}</p>}
    </section>
  )
}

/** `compact`: only the gear, for the folded rail. */
export function Settings({ compact = false, ws = null }: { compact?: boolean; ws?: Workspace | null }) {
  const { t, lang, setLang } = useT()
  const [theme, pickTheme] = useState<Theme>(readTheme)
  // Off until the user turns it on; the browser's permission is asked only then.
  const [notifying, setNotifying] = useState(notifyWanted)
  const [denied, setDenied] = useState(false)
  const item = 'h-7 px-3 text-xs font-normal data-[state=on]:font-semibold'
  return (
    <Dialog>
      <DialogTrigger asChild>
        {compact ? (
          <Button variant="ghost" size="icon-sm" aria-label={t('settings.title')} className="rounded-sm text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink">
            <Gear aria-hidden size={16} strokeWidth={1.5} />
          </Button>
        ) : (
          <Button variant="ghost" size="sm" className="justify-start gap-2 rounded-sm px-2 text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink">
            <Gear aria-hidden size={16} strokeWidth={1.5} />
            {t('settings.title')}
          </Button>
        )}
      </DialogTrigger>
      <DialogContent closeLabel={t('settings.close')}>
        <DialogHeader>
          <DialogTitle>{t('settings.title')}</DialogTitle>
        </DialogHeader>
        <div className="flex min-w-0 flex-col gap-3">
          <Row label={t('settings.language')}>
            <ToggleGroup type="single" variant="outline" size="sm" value={lang} onValueChange={v => v && setLang(v as Lang)} aria-label={t('settings.language')}>
              {LANGS.map(l => <ToggleGroupItem key={l.id} value={l.id} lang={l.id} className={item}>{l.label}</ToggleGroupItem>)}
            </ToggleGroup>
          </Row>
          <Row label={t('settings.theme')}>
            <ToggleGroup
              type="single" variant="outline" size="sm" value={theme} aria-label={t('settings.theme')}
              onValueChange={v => { if (v) { setTheme(v as Theme); pickTheme(v as Theme) } }}
            >
              {THEMES.map(id => <ToggleGroupItem key={id} value={id} className={item}>{t(`settings.themes.${id}`)}</ToggleGroupItem>)}
            </ToggleGroup>
          </Row>
          <Row label={t('settings.notify')}>
            <Switch
              checked={notifying}
              aria-label={t('settings.notify')}
              onCheckedChange={on => { wantNotify(on).then(ok => { setNotifying(ok); setDenied(on && !ok) }) }}
            />
          </Row>
          {denied && <p className="text-xs text-ink-2">{t('settings.notifyDenied')}</p>}
          {!MOCK && <Machines ws={ws} />}
        </div>
      </DialogContent>
    </Dialog>
  )
}
