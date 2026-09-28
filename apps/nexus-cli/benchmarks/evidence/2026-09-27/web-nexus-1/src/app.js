export const products = [
  { id: 'p1', name: 'Auriculares Pro', category: 'Audio', price: 99 },
  { id: 'p2', name: 'Teclado Mecánico', category: 'Oficina', price: 149 },
  { id: 'p3', name: 'Altavoz Mini', category: 'Audio', price: 39 },
  { id: 'p4', name: 'Soporte Laptop', category: 'Oficina', price: 29 },
];

function normalizeText(value) {
  return String(value ?? '').normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLocaleLowerCase('es');
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (character) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[character]));
}

export function selectProducts(items, { query = '', category = 'all', sort = 'name' } = {}) {
  const normalizedQuery = normalizeText(query).trim();
  const selected = items.filter((item) => {
    const matchesQuery = !normalizedQuery || normalizeText(item.name).includes(normalizedQuery);
    return matchesQuery && (category === 'all' || item.category === category);
  });
  return [...selected].sort((a, b) => {
    if (sort === 'price-asc') return Number(a.price) - Number(b.price);
    if (sort === 'price-desc') return Number(b.price) - Number(a.price);
    return String(a.name).localeCompare(String(b.name), 'es', { sensitivity: 'base' });
  });
}

export function renderProducts(items) {
  if (!items.length) return '<section class="empty" aria-live="polite"><h2>Sin resultados</h2><p>No hay productos que coincidan con tu búsqueda.</p></section>';
  const euros = new Intl.NumberFormat('es-ES', { style: 'currency', currency: 'EUR', maximumFractionDigits: 0 });
  return `<ul class="products" aria-label="Productos">${items.map((item) => `
    <li class="product" data-product-id="${escapeHtml(item.id)}">
      <div><p class="product__category">${escapeHtml(item.category)}</p><h2 class="product__name">${escapeHtml(item.name)}</h2></div>
      <p class="product__price">${euros.format(Number(item.price))}</p>
    </li>`).join('')}</ul>`;
}

function categoryOptions(items) {
  return [...new Set(items.map((item) => item.category))].sort((a, b) => a.localeCompare(b, 'es'))
    .map((category) => `<option value="${escapeHtml(category)}">${escapeHtml(category)}</option>`).join('');
}

function mountCatalog(root) {
  root.innerHTML = `
    <section class="catalog">
      <header class="catalog__header"><p class="eyebrow">Selección Nexo</p><h1 id="catalog-title">Encuentra lo que necesitas</h1><p class="catalog__intro">Explora accesorios elegidos para tu espacio de trabajo y tus momentos de audio.</p></header>
      <form class="filters" role="search" aria-label="Filtrar catálogo">
        <div class="field"><label for="search">Buscar productos</label><input id="search" name="search" type="search" placeholder="Ej. teclado o auriculares" autocomplete="off"></div>
        <div class="field"><label for="category">Categoría</label><select id="category" name="category"><option value="all">Todas las categorías</option>${categoryOptions(products)}</select></div>
        <div class="field"><label for="sort">Ordenar por</label><select id="sort" name="sort"><option value="name">Nombre (A–Z)</option><option value="price-asc">Precio: menor a mayor</option><option value="price-desc">Precio: mayor a menor</option></select></div>
      </form>
      <p id="results-count" class="catalog__results" aria-live="polite"></p><div id="product-list"></div>
    </section>`;
  const search = root.querySelector('#search');
  const category = root.querySelector('#category');
  const sort = root.querySelector('#sort');
  const list = root.querySelector('#product-list');
  const count = root.querySelector('#results-count');
  const update = () => {
    const visible = selectProducts(products, { query: search.value, category: category.value, sort: sort.value });
    count.textContent = visible.length === 1 ? '1 producto encontrado' : `${visible.length} productos encontrados`;
    list.innerHTML = renderProducts(visible);
  };
  search.addEventListener('input', update); category.addEventListener('change', update); sort.addEventListener('change', update); update();
}

if (typeof document !== 'undefined') {
  const root = document.querySelector('#app');
  if (root) mountCatalog(root);
}
