import { t } from './i18n'

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB']

export function fmtBytes(n?: number | null, digits = 1): string {
  if (n === undefined || n === null || Number.isNaN(n)) return '-'
  if (n < 1024) return `${n} B`
  let v = n
  let i = 0
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024
    i++
  }
  return `${v.toFixed(v >= 100 ? 0 : digits)} ${UNITS[i]}`
}

/**
 * 时长本地化。
 * 中文用「天/小时/分钟」这类汉字量词，英文用 d/h/min 后缀 —— 单位本身进文案表。
 */
export function fmtUptime(sec?: number): string {
  if (!sec && sec !== 0) return '-'
  const d = Math.floor(sec / 86400)
  const h = Math.floor((sec % 86400) / 3600)
  const m = Math.floor((sec % 3600) / 60)
  const parts: string[] = []
  if (d) parts.push(`${d} ${t('unit.day')}`)
  if (h) parts.push(`${h} ${t('unit.hour')}`)
  if (m || !parts.length) parts.push(`${m} ${t('unit.minute')}`)
  return parts.join(' ')
}

/** 毫秒 → 短时长。中文「分/秒」，英文 m/s。 */
export function fmtDuration(ms?: number | null): string {
  if (ms === undefined || ms === null) return '-'
  if (ms < 1000) return `${ms} ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`
  const m = Math.floor(ms / 60_000)
  const s = Math.floor((ms % 60_000) / 1000)
  return `${m} ${t('unit.minute')} ${s} ${t('unit.secShort')}`
}

export function fmtTime(ts?: number): string {
  if (!ts) return '-'
  const d = new Date(ts * 1000)
  if (Number.isNaN(d.getTime())) return '-'
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}

export function pct(used?: number, total?: number): number {
  if (!used || !total) return 0
  return Math.min(100, Math.round((used / total) * 1000) / 10)
}
