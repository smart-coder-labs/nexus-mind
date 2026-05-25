import React, { useState } from 'react';
import { Input } from '../ui/Input';
import { Button } from '../ui/Button';
import { Spinner } from '../ui/Spinner';

export default function WaitlistFormReact() {
  const [loading, setLoading] = useState(false);
  const [selectedInterests, setSelectedInterests] = useState<string[]>([]);

  const interests = [
    { value: 'coding', label: 'Asistente de código' },
    { value: 'agents', label: 'Agentes AI' },
    { value: 'memory', label: 'Sistema de memoria' },
    { value: 'governance', label: 'Gobierno enterprise' },
  ];

  const toggleInterest = (value: string) => {
    setSelectedInterests(prev =>
      prev.includes(value) ? prev.filter(i => i !== value) : [...prev, value]
    );
  };

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const form = e.currentTarget;
    const name = (form.elements.namedItem('name') as HTMLInputElement).value.trim();
    const email = (form.elements.namedItem('email') as HTMLInputElement).value.trim();
    const company = (form.elements.namedItem('company') as HTMLInputElement).value.trim();
    const size = (form.elements.namedItem('size') as HTMLSelectElement).value;
    const message = (form.elements.namedItem('message') as HTMLTextAreaElement).value.trim();

    if (!name || !email || !company) {
      (window as any).showToast?.('Completa todos los campos requeridos.', true);
      return;
    }

    setLoading(true);
    const entry = { name, email, company, size, interests: selectedInterests, message };

    try {
      if ((window as any).__nexusmindSubmit) {
        await (window as any).__nexusmindSubmit(entry);
      } else {
        const existing = JSON.parse(localStorage.getItem('nexusmind_waitlist') || '[]');
        existing.push({ ...entry, timestamp: new Date().toISOString() });
        localStorage.setItem('nexusmind_waitlist', JSON.stringify(existing));
      }
      (window as any).confetti?.();
      (window as any).showToast?.('¡Estás en la lista! Te avisaremos cuando tengamos slots.');
      form.reset();
      setSelectedInterests([]);
    } finally {
      setLoading(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="ds-surface p-8 space-y-5">
      <Input label="Nombre completo" name="name" required placeholder="e.g. Ana García" />
      <Input label="Email corporativo" name="email" type="email" required placeholder="ana@tuempresa.com" />
      <Input label="Empresa" name="company" required placeholder="Tu empresa S.L." />
      <div>
        <label className="block text-sm font-medium mb-1.5" style={{ color: 'var(--color-text-secondary)' }}>
          Tamaño del equipo
        </label>
        <div className="relative">
          <select
            name="size"
            className="w-full rounded-xl px-4 py-3 text-sm border focus:outline-none appearance-none cursor-pointer"
            style={{
              background: 'var(--color-bg-secondary)',
              borderColor: 'var(--color-border-primary)',
              color: 'var(--color-text-primary)',
            }}
          >
            <option value="1-10">1-10 personas</option>
            <option value="11-50">11-50 personas</option>
            <option value="51-200">51-200 personas</option>
            <option value="200+">200+ personas</option>
          </select>
          <div className="pointer-events-none absolute inset-y-0 right-3 flex items-center" style={{ color: 'var(--color-text-secondary)' }}>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </div>
        </div>
      </div>
      <div>
        <p className="text-sm font-medium mb-2" style={{ color: 'var(--color-text-secondary)' }}>
          ¿Qué te interesa más?
        </p>
        <div className="flex flex-wrap gap-2">
          {interests.map(({ value, label }) => (
            <button
              key={value}
              type="button"
              onClick={() => toggleInterest(value)}
              className="px-4 py-1.5 rounded-full text-sm border transition-all"
              style={{
                background: selectedInterests.includes(value) ? 'var(--color-accent-blue-tint)' : 'transparent',
                borderColor: selectedInterests.includes(value) ? 'var(--color-accent-blue)' : 'var(--color-border-primary)',
                color: selectedInterests.includes(value) ? 'var(--color-accent-blue)' : 'var(--color-text-secondary)',
              }}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      <div>
        <label className="block text-sm font-medium mb-1.5" style={{ color: 'var(--color-text-secondary)' }}>
          Mensaje (opcional)
        </label>
        <textarea
          name="message"
          rows={3}
          className="w-full rounded-xl px-4 py-3 text-sm border focus:outline-none resize-none"
          placeholder="Cuéntanos sobre tu stack actual..."
          style={{
            background: 'var(--color-bg-secondary)',
            borderColor: 'var(--color-border-primary)',
            color: 'var(--color-text-primary)',
          }}
        />
      </div>
      <Button type="submit" variant="primary" size="lg" fullWidth loading={loading}>
        Unirme a la lista
      </Button>
      <p className="text-xs text-center" style={{ color: 'var(--color-text-tertiary)' }}>
        No spam. Sin compromiso.
      </p>
    </form>
  );
}
