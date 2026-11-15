// Settings: the few choices that belong to the person, not to the work on screen.
import { useState } from 'react'
import { Settings as Gear } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { notifyWanted, wantNotify } from '@/runtime'
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

/** `compact`: only the gear, for the folded rail. */
export function Settings({ compact = false }: { compact?: boolean }) {
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
        <div className="flex flex-col gap-3">
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
        </div>
      </DialogContent>
    </Dialog>
  )
}
