export const products = [
  { id: 'p1', name: 'Auriculares Pro', category: 'Audio', price: 99 },
  { id: 'p2', name: 'Teclado Mecánico', category: 'Oficina', price: 149 },
  { id: 'p3', name: 'Altavoz Mini', category: 'Audio', price: 39 },
  { id: 'p4', name: 'Soporte Laptop', category: 'Oficina', price: 29 },
];

/** Normalize text for case- and accent-insensitive comparisons. */
function normalizeText(value) {
  return String(value ?? '')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLocaleLowerCase('es');
}

function compareNames(a, b) {
  return String(a.name ?? '').localeCompare(String(b.name ?? ''), 'es', { sensitivity: 'base' });
}

export function selectProducts(items, { query = '', category = 'all', sort = 'name' } = {}) {
  const normalizedQuery = normalizeText(query).trim();
  const filtered = items.filter((item) => {
    const matchesQuery = !normalizedQuery || normalizeText(item.name).includes(normalizedQuery);
    const matchesCategory = category === 'all' || item.category === category;
    return matchesQuery && matchesCategory;
  });

  const direction = sort.endsWith('-desc') ? -1 : 1;
  const sortByPrice = sort === 'price-asc' || sort === 'price-desc';
  return filtered.slice().sort((a, b) => {
    const result = sortByPrice
      ? Number(a.price) - Number(b.price)
      : compareNames(a, b);
    return result * direction;
  });
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>'"]/g, (character) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#39;', '"': '&quot;',
  })[character]);
}

function formatPrice(value) {
  const amount = Number(value);
  return new Intl.NumberFormat('es-ES', { style: 'currency', currency: 'EUR' })
    .format(Number.isFinite(amount) ? amount : 0);
}

export function renderProducts(items) {
  if (!items.length) {
    return '<div class="estado-vacio" role="status"><h2>Sin resultados</h2><p>No hay productos que coincidan con los filtros seleccionados.</p></div>';
  }
  return items.map((item) => `
    <article class="producto">
      <p class="producto__categoria">${escapeHtml(item.category)}</p>
      <h2 class="producto__nombre">${escapeHtml(item.name)}</h2>
      <p class="producto__precio">${escapeHtml(formatPrice(item.price))}</p>
    </article>`).join('');
}

if (typeof document !== 'undefined') {
  const app = document.querySelector('#app');
  const search = document.querySelector('#busqueda');
  const category = document.querySelector('#categoria');
  const order = document.querySelector('#orden');
  const summary = document.querySelector('#resumen');

  if (app && search && category && order && summary) {
    const categories = [...new Set(products.map((product) => product.category))];
    category.insertAdjacentHTML('beforeend', categories
      .map((value) => `<option value="${escapeHtml(value)}">${escapeHtml(value)}</option>`).join(''));

    const updateCatalog = () => {
      const selected = selectProducts(products, {
        query: search.value,
        category: category.value,
        sort: order.value,
      });
      app.innerHTML = renderProducts(selected);
      summary.textContent = `${selected.length} ${selected.length === 1 ? 'producto encontrado' : 'productos encontrados'}`;
    };

    search.addEventListener('input', updateCatalog);
    category.addEventListener('change', updateCatalog);
    order.addEventListener('change', updateCatalog);
    updateCatalog();
  }
}
