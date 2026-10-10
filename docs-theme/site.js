const navigation = document.querySelector('.navigation');
const mobile = window.matchMedia('(max-width: 760px)');
function updateNavigation() {
  navigation.open = !mobile.matches;
}
updateNavigation();
mobile.addEventListener('change', updateNavigation);

document.querySelectorAll('.document pre').forEach((block) => {
  const button = document.createElement('button');
  button.className = 'copy-code';
  button.type = 'button';
  button.textContent = 'Copy';
  button.setAttribute('aria-label', 'Copy code to clipboard');
  button.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(block.querySelector('code').textContent);
      button.textContent = 'Copied';
    } catch {
      button.textContent = 'Copy failed';
    }
    setTimeout(() => { button.textContent = 'Copy'; }, 2000);
  });
  if (block.querySelector('code')) block.append(button);
});
