<script lang="ts">
  import AppInput from '../../lib/components/ui/AppInput.svelte';
  import type { BeneficiaryFields } from './types';
  import { parseBeneficiaryCounts } from './preview';

  export let idPrefix: string;
  export let beneficiaries: BeneficiaryFields;
  export let disabled = false;

  // @category TransientState — derived from form props
  $: counts = parseBeneficiaryCounts(beneficiaries);
  // @category TransientState — summed from form counts
  $: totalBeneficiaries =
    counts.staff_24h_count +
    counts.staff_8h_count +
    counts.reservation_count +
    counts.mission_count +
    counts.guest_count;
</script>

<div class="grid grid-cols-2 md:grid-cols-3 gap-4">
  <AppInput
    id="{idPrefix}-staff24h"
    label="عناصر 24س/48ر"
    type="number"
    placeholder="0"
    bind:value={beneficiaries.staff24h}
    {disabled}
  />
  <AppInput
    id="{idPrefix}-staff8h"
    label="عناصر 8 ساعات"
    type="number"
    placeholder="0"
    bind:value={beneficiaries.staff8h}
    {disabled}
  />
  <AppInput
    id="{idPrefix}-reservation"
    label="محجوزون"
    type="number"
    placeholder="0"
    bind:value={beneficiaries.reservation}
    {disabled}
  />
  <AppInput
    id="{idPrefix}-mission"
    label="مهمة"
    type="number"
    placeholder="0"
    bind:value={beneficiaries.mission}
    {disabled}
  />
  <AppInput
    id="{idPrefix}-guest"
    label="ضيوف"
    type="number"
    placeholder="0"
    bind:value={beneficiaries.guest}
    {disabled}
  />
</div>

<p class="mt-3 text-sm text-gray-600 dark:text-gray-400">
  إجمالي المستفيدين (معاينة): <strong class="text-gray-900 dark:text-gray-100">{totalBeneficiaries}</strong>
</p>
