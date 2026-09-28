export const products = [
  { id: 'p1', name: 'Auriculares Pro', category: 'Audio', price: 99 },
  { id: 'p2', name: 'Teclado Mecánico', category: 'Oficina', price: 149 },
  { id: 'p3', name: 'Altavoz Mini', category: 'Audio', price: 39 },
  { id: 'p4', name: 'Soporte Laptop', category: 'Oficina', price: 29 },
];

const collator = new Intl.Collator('es', { sensitivity: 'base' });
const normalize = (value) => String(value ?? '').normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLocaleLowerCase('es').trim();
const escapeHtml = (value) => String(value ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
const formatPrice = (price) => new Intl.NumberFormat('es-ES', { style: 'currency', currency: 'EUR', maximumFractionDigits: 0 }).format(Number(price) || 0);

export function selectProducts(items, { query = '', category = 'all', sort = 'name' } = {}) {
  const normalizedQuery = normalize(query);
  const normalizedCategory = normalize(category);
  const filtered = items.filter((item) => (!normalizedQuery || normalize(item.name).includes(normalizedQuery)) && (normalizedCategory === 'all' || normalize(item.category) === normalizedCategory));
  return [...filtered].sort((left, right) => {
    if (sort === 'price-asc') return Number(left.price) - Number(right.price);
    if (sort === 'price-desc') return Number(right.price) - Number(left.price);
    return (sort === 'name-desc' ? -1 : 1) * collator.compare(String(left.name), String(right.name));
  });
}

export function renderProducts(items) {
  if (!items.length) return '<div class="vacio" role="status"><h3>Sin resultados</h3><p>Prueba con otra búsqueda o cambia la categoría.</p></div>';
  return items.map((item) => `<article class="producto"><p class="producto__categoria">${escapeHtml(item.category)}</p><h3>${escapeHtml(item.name)}</h3><p class="producto__precio">${escapeHtml(formatPrice(item.price))}</p></article>`).join('');
}

function initializeCatalog() {
  const searchInput = document.querySelector('#busqueda');
  const categorySelect = document.querySelector('#categoria');
  const sortSelect = document.querySelector('#orden');
  const productList = document.querySelector('#productos');
  const counter = document.querySelector('#contador');
  if (!searchInput || !categorySelect || !sortSelect || !productList || !counter) return;

  const categories = [...new Set(products.map((product) => product.category))].sort(collator.compare);
  categorySelect.innerHTML = ['<option value="all">Todas las categorías</option>', ...categories.map((category) => `<option value="${escapeHtml(category)}">${escapeHtml(category)}</option>`)].join('');
  const update = () => {
    const selected = selectProducts(products, { query: searchInput.value, category: categorySelect.value, sort: sortSelect.value });
    productList.innerHTML = renderProducts(selected);
    counter.textContent = `${selected.length} ${selected.length === 1 ? 'producto' : 'productos'}`;
  };
  searchInput.addEventListener('input', update);
  categorySelect.addEventListener('change', update);
  sortSelect.addEventListener('change', update);
  update();
}

if (typeof document !== 'undefined') initializeCatalog();
