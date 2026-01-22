console.log('Testing electron require...');
const electronPath = require('electron');
console.log('electron module returned:', typeof electronPath, electronPath);

// Try to access app directly 
try {
    const { app } = require('electron');
    console.log('app:', typeof app);
} catch (e) {
    console.log('Error accessing app:', e.message);
}
