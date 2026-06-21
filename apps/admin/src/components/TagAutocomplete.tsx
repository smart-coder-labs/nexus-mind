import React, { useState, useRef, useEffect, useCallback } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'

interface TagAutocompleteProps {
  value: string
  onChange: (value: string) => void
  onSelect: (tag: string) => void
  onKeyDown?: (e: React.KeyboardEvent) => void
  placeholder?: string
  className?: string
  existingTags?: string[]
}

export function TagAutocomplete({
  value,
  onChange,
  onSelect,
  onKeyDown,
  placeholder,
  className,
  existingTags = [],
}: TagAutocompleteProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const [highlightedIndex, setHighlightedIndex] = useState(-1)
  const [open, setOpen] = useState(false)
  const containerRef = useRef<HTMLDivElement>(null)

  const { data: tagStats } = useQuery({
    queryKey: ['tag-stats'],
    queryFn: () => client.getTagStats(),
    staleTime: 60_000,
  })

  const suggestions = useMemo(() => {
    if (!value || value.length < 1 || !tagStats) return []
    const lower = value.toLowerCase()
    return tagStats
      .filter(t => t.name.toLowerCase().startsWith(lower) && !existingTags.includes(t.name))
      .slice(0, 6)
  }, [value, tagStats, existingTags])

  const isOpen = open && suggestions.length > 0

  // Reset highlighted index when suggestions change
  useEffect(() => {
    setHighlightedIndex(-1)
  }, [suggestions.length])

  // Close on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (isOpen) {
        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setHighlightedIndex(i => Math.min(i + 1, suggestions.length - 1))
          return
        }
        if (e.key === 'ArrowUp') {
          e.preventDefault()
          setHighlightedIndex(i => Math.max(i - 1, -1))
          return
        }
        if (e.key === 'Enter' && highlightedIndex >= 0) {
          e.preventDefault()
          onSelect(suggestions[highlightedIndex].name)
          setOpen(false)
          return
        }
        if (e.key === 'Escape') {
          setOpen(false)
          return
        }
      }
      onKeyDown?.(e)
    },
    [isOpen, highlightedIndex, suggestions, onSelect, onKeyDown],
  )

  return (
    <div ref={containerRef} className="relative">
      <input
        type="text"
        value={value}
        onChange={e => {
          onChange(e.target.value)
          setOpen(true)
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        className={className}
        autoComplete="off"
      />
      {isOpen && (
        <div className="bg-[#272729] border border-border-primary rounded-[11px] py-1 shadow-xl absolute z-50 w-full mt-1">
          {suggestions.map((stat, i) => (
            <button
              key={stat.name}
              type="button"
              onMouseDown={e => {
                e.preventDefault()
                onSelect(stat.name)
                setOpen(false)
              }}
              onMouseEnter={() => setHighlightedIndex(i)}
              className={`w-full px-3 py-1.5 text-xs cursor-pointer flex items-center justify-between text-left ${
                i === highlightedIndex
                  ? 'bg-white/[0.06] text-text-primary'
                  : 'text-text-secondary hover:bg-white/[0.04]'
              }`}
            >
              <span>{stat.name}</span>
              <span className="text-[10px] text-text-quaternary">{stat.count}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
