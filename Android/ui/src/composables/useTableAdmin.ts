import { onMounted, ref } from 'vue'
import { api, formatMoney, type ProductHit, type ServiceTable, type TableCreateSpec } from '../api'

/** 台位管理页共用的数据逻辑；各行业的页面模板与文案各自独立。 */
export function useTableAdmin(scene: 'DINING' | 'CHESS' | 'WELLNESS', withFee: boolean, noun: string) {
  const tables = ref<ServiceTable[]>([])
  const feeProducts = ref<ProductHit[]>([])
  const busy = ref(false)
  const showCreate = ref(false)
  const err = ref('')

  async function reload() {
    try {
      tables.value = await api.listServiceTables(scene)
      err.value = ''
    } catch (e) {
      err.value = String(e)
    }
  }

  async function loadFees() {
    if (!withFee) return
    try {
      feeProducts.value = await api.lookup('', 200, '房费')
    } catch {
      feeProducts.value = []
    }
  }

  function feeText(t: ServiceTable): string {
    if (!withFee) return '—'
    const fee = feeProducts.value.find((p) => p.sku_id === t.fee_sku_id)
    if (!fee) return '未绑定房费'
    const unit = t.fee_unit === 'HOUR' ? '/小时' : t.fee_unit === 'MINUTE' ? '/分钟' : ''
    return `${fee.spu_name} ¥${formatMoney(fee.price_cents)}${unit}`
  }

  async function createBatch(items: TableCreateSpec[]) {
    if (busy.value) return
    busy.value = true
    try {
      let done = 0
      for (const it of items) {
        try {
          await api.createServiceTable(
            scene, it.code, it.name, it.zone, it.seats,
            it.feeSkuId, it.feeUnit, it.minMinutes, it.roundMinutes
          )
          done++
        } catch (e) {
          err.value = done ? `已创建 ${done}/${items.length}，${it.code} 失败：${String(e)}` : String(e)
          break
        }
      }
      showCreate.value = false
      await reload()
    } finally {
      busy.value = false
    }
  }

  async function remove(t: ServiceTable) {
    if (t.status !== 'FREE') {
      err.value = `${t.code} 正在使用中，请先清台或结算后再删除`
      return
    }
    if (!window.confirm(`删除空${noun} ${t.code}？`)) return
    busy.value = true
    try {
      await api.deleteServiceTable(t.table_id)
      err.value = ''
      await reload()
    } catch (e) {
      err.value = String(e)
    } finally {
      busy.value = false
    }
  }

  async function clear(t: ServiceTable) {
    if (!window.confirm(`未结算清台 ${t.code}？购物车会丢弃，本操作留审计。`)) return
    busy.value = true
    try {
      await api.closeServiceTable(t.table_id, true)
      err.value = ''
      await reload()
    } catch (e) {
      err.value = String(e)
    } finally {
      busy.value = false
    }
  }

  onMounted(() => {
    void reload()
    void loadFees()
  })

  return { tables, feeProducts, busy, showCreate, err, feeText, createBatch, remove, clear, reload }
}
