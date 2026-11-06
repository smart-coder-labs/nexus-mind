export const products = [
  { id: 'p1', name: 'Auriculares Pro', category: 'Audio', price: 99 },
  { id: 'p2', name: 'Teclado Mecánico', category: 'Oficina', price: 149 },
  { id: 'p3', name: 'Altavoz Mini', category: 'Audio', price: 39 },
  { id: 'p4', name: 'Soporte Laptop', category: 'Oficina', price: 29 },
];

export function selectProducts(items, { query = '', category = 'all', sort = 'name' } = {}) {
  const normalize = (value) => String(value ?? '').normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLocaleLowerCase('es');
  const term = normalize(query).trim();
  const selectedCategory = normalize(category);
  const visible = items.filter((item) => (!term || normalize(item.name).includes(term)) && (selectedCategory === 'all' || normalize(item.category) === selectedCategory));
  const direction = sort === 'name-desc' || sort === 'price-desc' ? -1 : 1;
  return [...visible].sort((left, right) => (sort === 'price-asc' || sort === 'price-desc')
    ? direction * (Number(left.price) - Number(right.price))
    : direction * String(left.name).localeCompare(String(right.name), 'es', { sensitivity: 'base' }));
}

export function renderProducts(items) {
  const escapeHtml = (value) => String(value ?? '').replace(/[&<>'"]/g, (character) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#39;', '"': '&quot;' }[character]));
  if (!items.length) return '<div class="empty" role="status"><h2>Sin resultados</h2><p>Prueba con otra búsqueda o categoría.</p></div>';
  return items.map((item) => {
    const price = Number(item.price);
    const label = Number.isFinite(price) ? price.toLocaleString('es-ES', { style: 'currency', currency: 'EUR' }) : 'Precio no disponible';
    return `<article class="product" data-product-id="${escapeHtml(item.id)}"><p class="product__category">${escapeHtml(item.category)}</p><h2>${escapeHtml(item.name)}</h2><p class="product__price">${escapeHtml(label)}</p></article>`;
  }).join('');
}

if (typeof document !== 'undefined') {
  const productContainer = document.querySelector('#products');
  const search = document.querySelector('#search');
  const category = document.querySelector('#category');
  const sort = document.querySelector('#sort');
  const count = document.querySelector('#results-count');
  if (productContainer && search && category && sort && count) {
    const escapeHtml = (value) => String(value).replace(/[&<>'"]/g, (character) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#39;', '"': '&quot;' }[character]));
    const categories = [...new Set(products.map((item) => item.category))].sort((a, b) => a.localeCompare(b, 'es'));
    category.innerHTML = `<option value="all">Todas las categorías</option>${categories.map((name) => `<option value="${escapeHtml(name)}">${escapeHtml(name)}</option>`).join('')}`;
    const update = () => { const selected = selectProducts(products, { query: search.value, category: category.value, sort: sort.value }); productContainer.innerHTML = renderProducts(selected); count.textContent = `${selected.length} ${selected.length === 1 ? 'producto encontrado' : 'productos encontrados'}`; };
    search.addEventListener('input', update); category.addEventListener('change', update); sort.addEventListener('change', update); update();
  }
}
