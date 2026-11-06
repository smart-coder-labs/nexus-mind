import { useEffect, useRef, type RefObject } from 'react'
import { initLoginMesh } from './hero-mesh'
import './LoginBackground.css'

export function LoginBackground({ eventRootRef }: { eventRootRef: RefObject<HTMLDivElement | null> }) {
  const rootRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const cursorRef = useRef<HTMLSpanElement>(null)

  useEffect(() => {
    const root = rootRef.current
    const canvas = canvasRef.current
    const cursor = cursorRef.current
    if (!root || !canvas || !cursor || typeof CanvasRenderingContext2D === 'undefined') return
    return initLoginMesh(root, canvas, cursor, eventRootRef.current ?? root)
  }, [eventRootRef])

  return (
    <>
      <div ref={rootRef} className="login-mesh-layer" aria-hidden="true">
        <canvas ref={canvasRef} className="login-mesh-canvas" />
        <div className="login-mesh-shade" />
      </div>
      <span ref={cursorRef} className="login-mesh-cursor" aria-hidden="true"><span /></span>
    </>
  )
}
