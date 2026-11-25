import { onMounted, onUnmounted, ref } from 'vue'
import { api, type CartLine, type ProductHit, type ServiceTable, type TableCreateSpec } from '../api'

/** 棋牌包间 / 养生服务间共用的数据逻辑；页面与视觉各自独立，不共用页面组件。 */
export function useServiceRooms(opts: {
  scene: 'CHESS' | 'WELLNESS'
  roomNoun: string
  activeTableId: () => number | null
  cart: () => CartLine[]
  memberId: () => number | null
  onSelect: (table: ServiceTable, lines: CartLine[]) => void
  onCleared: () => void
  onError: (message: string) => void
}) {
  const rooms = ref<ServiceTable[]>([])
  const feeProducts = ref<ProductHit[]>([])
  const busy = ref(false)
  const showCreate = ref(false)
  const transferSource = ref<ServiceTable | null>(null)
  const tick = ref(Date.now())
  let timer: ReturnType<typeof setInterval> | null = null

  async function load() {
    try {
      rooms.value = await api.listServiceTables(opts.scene)
    } catch (e) {
      opts.onError(String(e))
    }
  }

  async function loadFees() {
    try {
      feeProducts.value = await api.lookup('', 200, '房费')
    } catch (e) {
      opts.onError(String(e))
    }
  }

  function feeOf(t: ServiceTable): ProductHit | null {
    return feeProducts.value.find((p) => p.sku_id === t.fee_sku_id) ?? null
  }

  function linesOf(t: ServiceTable): CartLine[] {
    if (t.status !== 'OCCUPIED') return []
    try {
      const parsed = JSON.parse(t.cart_json)
      return Array.isArray(parsed) ? parsed : []
    } catch {
      return []
    }
  }

  function elapsed(t: ServiceTable): string {
    if (!t.opened_at) return '00:00'
    const sec = Math.max(0, Math.floor((tick.value - new Date(t.opened_at).getTime()) / 1000))
    const h = String(Math.floor(sec / 3600)).padStart(2, '0')
    const m = String(Math.floor((sec % 3600) / 60)).padStart(2, '0')
    const s = String(sec % 60).padStart(2, '0')
    return `${h}:${m}:${s}`
  }

  function itemCount(t: ServiceTable): number {
    if (t.table_id === opts.activeTableId()) return opts.cart().reduce((n, l) => n + l.qty, 0)
    return linesOf(t).reduce((n, l) => n + (l.qty || 0), 0)
  }

  async function pick(t: ServiceTable) {
    if (busy.value) return
    if (transferSource.value) {
      await runTransfer(t)
      return
    }
    busy.value = true
    try {
      if (t.status === 'FREE') {
        if (!feeOf(t)) {
          opts.onError(`该${opts.roomNoun}未绑定房费商品，请先删除后重建并选择房费项目`)
          return
        }
        const opened = await api.openServiceTable(t.table_id, '[]', opts.memberId())
        opts.onSelect(opened, linesOf(opened))
      } else {
        opts.onSelect(t, linesOf(t))
      }
      await load()
    } catch (e) {
      opts.onError(String(e))
    } finally {
      busy.value = false
    }
  }

  async function runTransfer(target: ServiceTable) {
    const source = transferSource.value
    if (!source) return
    if (target.status !== 'FREE') {
      opts.onError(`转${opts.roomNoun}请选择空闲${opts.roomNoun}`)
      return
    }
    if (!window.confirm(`转${opts.roomNoun}：${source.code} → ${target.code}？计时与消费继续保留。`)) return
    busy.value = true
    try {
      const updated = await api.transferServiceTable(source.table_id, target.table_id)
      transferSource.value = null
      opts.onSelect(updated, linesOf(updated))
      await load()
    } catch (e) {
      opts.onError(String(e))
    } finally {
      busy.value = false
    }
  }

  function startTransfer(t: ServiceTable) {
    transferSource.value = t
  }

  async function createBatch(items: TableCreateSpec[]) {
    if (busy.value) return
    busy.value = true
    try {
      let done = 0
      for (const it of items) {
        try {
          const tableId = await api.createServiceTable(
            opts.scene, it.code, it.name, it.zone, it.seats,
            it.feeSkuId, it.feeUnit, it.minMinutes, it.roundMinutes
          )
          if (it.capCents && it.capCents > 0) {
            await api.setServiceTableFeeCap(tableId, it.capCents)
          }
          done++
        } catch (e) {
          opts.onError(done ? `已创建 ${done}/${items.length}，${it.code} 失败：${String(e)}` : String(e))
          break
        }
      }
      showCreate.value = false
      await load()
    } finally {
      busy.value = false
    }
  }

  async function clear(t: ServiceTable) {
    if (!window.confirm(`未结算清台 ${t.code}？本操作留审计。`)) return
    busy.value = true
    try {
      await api.closeServiceTable(t.table_id, true)
      if (t.table_id === opts.activeTableId()) opts.onCleared()
      await load()
    } catch (e) {
      opts.onError(String(e))
    } finally {
      busy.value = false
    }
  }

  onMounted(() => {
    void load()
    void loadFees()
    timer = setInterval(() => (tick.value = Date.now()), 1000)
  })
  onUnmounted(() => {
    if (timer) clearInterval(timer)
  })

  return {
    rooms, feeProducts, busy, showCreate, transferSource, tick,
    feeOf, linesOf, elapsed, itemCount,
    pick, startTransfer, createBatch, clear, load,
  }
}
