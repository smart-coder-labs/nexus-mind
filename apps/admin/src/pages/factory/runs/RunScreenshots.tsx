import { useState } from 'react'
import { ChevronLeft, ChevronRight, ExternalLink } from 'lucide-react'
import { Button } from '../../../components/ui/Button'
import {
  Modal,
  ModalCloseButton,
  ModalContent,
  ModalDescription,
  ModalFooter,
  ModalHeader,
  ModalTitle,
} from '../../../components/ui/Modal'
import { TEXT_LINK } from '../shared/ui'

const str = (value: unknown) => (typeof value === 'string' && value.trim() ? value : undefined)

/** Every screenshot of a run, browsable in a modal next to the finding it backs. */
export function RunScreenshots({
  runId,
  screenshots,
  findings = [],
}: {
  runId?: string
  screenshots: Record<string, unknown>
  findings?: unknown[]
}) {
  const shots = Object.entries(screenshots).map(([name, url]) => ({
    name,
    // Prefer the stable re-signing endpoint (durable) over the raw presigned
    // URL baked into the result, which expires after 7 days.
    src: runId
      ? `${import.meta.env.VITE_API_URL ?? ''}/evidence/${encodeURIComponent(runId)}/${encodeURIComponent(name)}`
      : str(url),
    finding: findings.find(
      (finding) => str((finding as Record<string, unknown> | null)?.screenshot) === name,
    ) as Record<string, unknown> | undefined,
  }))
  const [open, setOpen] = useState<number | null>(null)
  const current = open == null ? undefined : shots[open]
  const go = (step: number) =>
    setOpen((index) => (index == null ? index : (index + step + shots.length) % shots.length))

  return (
    <>
      <div className="flex flex-wrap gap-2.5">
        {shots.map((shot, index) => (
          <button
            key={shot.name}
            type="button"
            aria-label={`Open screenshot ${shot.name}`}
            onClick={() => setOpen(index)}
            className="w-28 rounded-md text-left focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring"
          >
            {shot.src ? (
              <img
                src={shot.src}
                alt=""
                loading="lazy"
                className="h-[70px] w-full rounded-md border border-border-primary object-cover object-top transition-opacity hover:opacity-80"
              />
            ) : (
              <div className="h-[70px] rounded-md border border-border-primary bg-foreground/[0.03]" />
            )}
            <div className="mt-1 truncate text-center text-[11px] text-text-tertiary">{shot.name}</div>
          </button>
        ))}
      </div>
      <Modal
        open={current != null}
        onOpenChange={(value) => {
          if (!value) setOpen(null)
        }}
        size="full"
      >
        {current && (
          <div
            className="flex min-h-0 flex-col"
            onKeyDown={(event) => {
              if (event.key === 'ArrowRight') go(1)
              if (event.key === 'ArrowLeft') go(-1)
            }}
          >
            <ModalCloseButton />
            <ModalHeader>
              <ModalTitle>{str(current.finding?.title) ?? current.name}</ModalTitle>
              <ModalDescription>
                {[str(current.finding?.severity), current.name, `${(open ?? 0) + 1} / ${shots.length}`]
                  .filter(Boolean)
                  .join(' · ')}
              </ModalDescription>
            </ModalHeader>
            <ModalContent className="space-y-3">
              {current.src ? (
                <img
                  src={current.src}
                  alt={`Screenshot ${current.name}`}
                  className="mx-auto max-h-[65dvh] w-auto max-w-full rounded-md border border-border-primary object-contain"
                />
              ) : (
                <p className="text-sm text-text-tertiary">This screenshot is no longer available.</p>
              )}
              {str(current.finding?.summary) && (
                <p className="text-sm leading-relaxed text-text-secondary">{str(current.finding?.summary)}</p>
              )}
            </ModalContent>
            <ModalFooter className="items-center justify-between">
              {current.src ? (
                <a href={current.src} target="_blank" rel="noreferrer" className={`inline-flex items-center gap-1 ${TEXT_LINK}`}>
                  Open original <ExternalLink className="h-3.5 w-3.5" aria-hidden />
                </a>
              ) : (
                <span />
              )}
              {shots.length > 1 && (
                <div className="flex gap-2">
                  <Button size="sm" variant="secondary" aria-label="Previous screenshot" onClick={() => go(-1)}>
                    <ChevronLeft className="h-4 w-4" aria-hidden />
                  </Button>
                  <Button size="sm" variant="secondary" aria-label="Next screenshot" onClick={() => go(1)}>
                    <ChevronRight className="h-4 w-4" aria-hidden />
                  </Button>
                </div>
              )}
            </ModalFooter>
          </div>
        )}
      </Modal>
    </>
  )
}
